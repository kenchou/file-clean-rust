use std::collections::{HashMap, HashSet};
use std::fs::rename;
use std::path::PathBuf;
use std::sync::Arc;

use colored::*;
use rayon::prelude::*;
use walkdir::WalkDir;

mod cli;
mod data;
mod fnmatch_regex;
mod p2tree;
mod pconfig;
mod pmatcher;
mod tprint;
mod util;

fn main() -> std::io::Result<()> {
    let app_options = cli::parse()?;

    if app_options.is_debug_mode() {
        println!("{:#?}", app_options);
    }

    let pattern_matcher = Arc::new(pmatcher::PatternMatcher::from_config_file(
        &app_options.config_file,
    ));
    if app_options.is_debug_mode() {
        println!("{:#?}", pattern_matcher);
    }

    // 仅扫描一次文件系统，收集所有路径
    let entries: Vec<_> = WalkDir::new(&app_options.target_path)
        .sort_by(|a, b| {
            let depth_a = a.depth();
            let depth_b = b.depth();
            depth_b
                .cmp(&depth_a)
                .then(
                    a.file_type()
                        .is_dir()
                        .cmp(&b.file_type().is_dir())
                        .reverse(),
                )
                .then(a.file_name().cmp(b.file_name()))
        })
        .into_iter()
        .filter_entry(|e| !app_options.skip_parent_tmp || util::is_not_hidden(e))
        .filter_map(|e| e.ok())
        .collect();

    // 并行处理文件信息
    let options_ref = &app_options;
    let matcher_ref = &pattern_matcher;

    let file_info_results: Vec<_> = entries
        .par_iter()
        .filter_map(|entry| {
            let filepath = entry.path();
            // 处理无效文件名：输出警告并跳过
            let filename = match entry.file_name().to_str() {
                Some(name) => name,
                None => {
                    eprintln!("{} 跳过无效文件名: {:?}", "[警告]".yellow(), filepath);
                    return None; // 跳过这个条目
                }
            };

            // 检查是否需要删除
            if options_ref.enable_deletion {
                let (mut matched, mut pattern) = matcher_ref.match_remove_pattern(filename);
                if matched {
                    let p = pattern.unwrap();
                    return Some((filepath.to_path_buf(), (p, data::Operation::Delete)));
                } else if options_ref.enable_hash_matching {
                    (matched, pattern) = matcher_ref.match_remove_hash(filepath.to_str().unwrap());
                    if matched {
                        let p = pattern.unwrap();
                        return Some((filepath.to_path_buf(), (p, data::Operation::Delete)));
                    }
                }
            }

            // 检查是否需要重命名
            if options_ref.enable_renaming {
                let new_filename = matcher_ref.clean_filename(filename);
                if new_filename != filename {
                    return Some((
                        filepath.to_path_buf(),
                        (new_filename, data::Operation::Rename),
                    ));
                }
            }

            // 检查是否为空目录
            if options_ref.enable_prune_empty_dir && filepath.is_dir() {
                if filepath
                    .read_dir()
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(false)
                {
                    return Some((
                        filepath.to_path_buf(),
                        ("<EMPTY_DIR>".to_string(), data::Operation::Delete),
                    ));
                }
            }

            // 不需要操作的文件
            Some((
                filepath.to_path_buf(),
                ("".to_string(), data::Operation::None),
            ))
        })
        .collect();

    // 构建文件信息映射
    let mut file_info: HashMap<PathBuf, (String, data::Operation)> = HashMap::new();
    let mut all_paths: Vec<PathBuf> = Vec::with_capacity(file_info_results.len());

    for (path, info) in file_info_results {
        all_paths.push(path.clone());
        file_info.insert(path, info);
    }

    // 构建操作列表
    let operation_list: Vec<(PathBuf, String, data::Operation)> = file_info
        .iter()
        .map(|(path, (pattern, op))| (path.clone(), pattern.clone(), op.clone()))
        .collect();

    if app_options.is_debug_mode() {
        println!("* operation_list: {:#?}", operation_list);
    }

    // 打印目录树
    if app_options.verbose >= 2 {
        tprint::print_tree(p2tree::path_list_to_tree(
            &operation_list,
            &app_options.target_path,
        ));
    }

    // 处理递归的空目录删除 - 优化算法
    if app_options.enable_deletion && app_options.enable_prune_empty_dir {
        let mut to_delete: HashSet<PathBuf> = file_info
            .iter()
            .filter(|(_, (_, op))| *op == data::Operation::Delete)
            .map(|(path, _)| path.clone())
            .collect();

        // 构建目录树结构
        let mut dir_children: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();

        for path in all_paths.iter() {
            if path.is_dir() {
                dir_children.insert(path.clone(), Vec::new());
            }

            // 找到父目录并添加为子项
            if let Some(parent) = path.parent().map(|p| p.to_path_buf()) {
                if all_paths.contains(&parent) && !to_delete.contains(path) {
                    dir_children
                        .entry(parent)
                        .or_insert_with(Vec::new)
                        .push(path.clone());
                }
            }
        }

        // 查找空目录 - 从不包含其他目录的目录开始
        // 使用 capacity 预分配容器大小
        let mut empty_dirs = Vec::with_capacity(dir_children.len() / 2);
        let mut changed = true;

        while changed {
            changed = false;

            for (dir, children) in &dir_children {
                if !to_delete.contains(dir) && children.is_empty() {
                    empty_dirs.push(dir.clone());
                    changed = true;
                }
            }

            // 将空目录标记为删除
            for dir in &empty_dirs {
                file_info.insert(
                    dir.clone(),
                    ("<EMPTY_DIR>".to_string(), data::Operation::Delete),
                );
                to_delete.insert(dir.clone());

                // 从父目录的子列表中移除
                if let Some(parent) = dir.parent().map(|p| p.to_path_buf()) {
                    if let Some(siblings) = dir_children.get_mut(&parent) {
                        siblings.retain(|p| p != dir);
                    }
                }
            }

            if !empty_dirs.is_empty() {
                empty_dirs.clear();
            } else {
                break;
            }
        }
    }

    // 执行删除操作
    if app_options.enable_deletion {
        file_info
            .iter()
            .filter(|(_, (_, op))| *op == data::Operation::Delete)
            .for_each(|(file_path, (pattern, _))| {
                // 删除操作代码...
                if app_options.verbose > 0 {
                    println!("{} {:#?} <== {}", "[-]".red(), file_path, pattern);
                } else {
                    println!("{} {:#?}", "[-]".red(), file_path);
                }

                if app_options.prune {
                    match util::remove_path(file_path.clone()) {
                        Ok(_) => (),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                        Err(e) => {
                            eprintln!("{} 删除文件失败 {:?}: {}", "[错误]".red(), file_path, e)
                        }
                    }
                }
            });
    }

    // 执行重命名操作
    if app_options.enable_renaming {
        let rename_operations: Vec<(PathBuf, String)> = file_info
            .iter()
            .filter(|(_, (_, op))| *op == data::Operation::Rename)
            .map(|(path, (pattern, _))| (path.clone(), pattern.clone()))
            .collect();

        for (file_path, new_file_name) in rename_operations {
            println!("{} {:#?} ==> {}", "[*]".yellow(), file_path, new_file_name);
            let mut new_filepath = file_path.clone();
            new_filepath.set_file_name(&new_file_name);
            if app_options.prune {
                println!("--> {}", new_filepath.display().to_string().cyan());
                rename(file_path, new_filepath)?;
            }
        }
    }

    Ok(())
}
