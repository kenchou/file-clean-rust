use std::collections::{HashMap, HashSet};
use std::fs::rename;
use std::path::PathBuf;

use colored::*;
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

    let pattern_matcher = pmatcher::PatternMatcher::from_config_file(&app_options.config_file);
    if app_options.is_debug_mode() {
        println!("{:#?}", pattern_matcher);
    }

    // 仅扫描一次文件系统，收集所有信息
    let mut file_info: HashMap<PathBuf, (String, data::Operation)> = HashMap::new();
    let mut all_paths: Vec<PathBuf> = Vec::new();

    // 第一次扫描文件系统，收集所有文件和目录信息
    for entry in WalkDir::new(&app_options.target_path)
        .sort_by(|a, b| {
            // 深度优先排序，从深到浅（先处理最深层级）
            let depth_a = a.depth();
            let depth_b = b.depth();
            depth_b.cmp(&depth_a)
                // 若深度相同，目录排在文件前面
                .then(a.file_type().is_dir().cmp(&b.file_type().is_dir()).reverse())
                // 若都是目录或都是文件，按名称排序
                .then(a.file_name().cmp(b.file_name()))
        })
        .into_iter()
        .filter_entry(|e| !app_options.skip_parent_tmp || util::is_not_hidden(e))
        .filter_map(|e| e.ok())
    {
        let filepath = entry.path();
        let filename = entry.file_name().to_str().unwrap();
        all_paths.push(filepath.to_path_buf());

        // 检查是否需要删除
        if app_options.enable_deletion {
            let (mut matched, mut pattern) = pattern_matcher.match_remove_pattern(filename);
            if matched {
                let p = pattern.unwrap();
                file_info.insert(filepath.to_path_buf(), (p, data::Operation::Delete));
                continue;
            } else if app_options.enable_hash_matching {
                // 只在必要时计算哈希
                (matched, pattern) = pattern_matcher.match_remove_hash(filepath.to_str().unwrap());
                if matched {
                    let p = pattern.unwrap();
                    file_info.insert(filepath.to_path_buf(), (p, data::Operation::Delete));
                    continue;
                }
            }
        }

        // 检查是否需要重命名
        if app_options.enable_renaming {
            let new_filename = pattern_matcher.clean_filename(filename);
            if new_filename != filename {
                file_info.insert(filepath.to_path_buf(), (new_filename, data::Operation::Rename));
                continue;
            }
        }

        // 检查是否为空目录
        if app_options.enable_prune_empty_dir
            && filepath.is_dir()
            && filepath.read_dir()?.next().is_none()
        {
            file_info.insert(filepath.to_path_buf(), ("<EMPTY_DIR>".to_string(), data::Operation::Delete));
            continue;
        }

        // 不需要操作的文件
        file_info.insert(filepath.to_path_buf(), ("".to_string(), data::Operation::None));
    }

    // 将 HashMap 转换为 Vec 用于打印目录树
    let operation_list: Vec<(PathBuf, String, data::Operation)> = file_info
        .iter()
        .map(|(path, (pattern, op))| (path.clone(), pattern.clone(), (*op).clone()))
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

    // 处理递归的空目录删除
    if app_options.enable_deletion && app_options.enable_prune_empty_dir {
        // 创建待删除路径集合
        let mut to_delete: HashSet<PathBuf> = file_info
            .iter()
            .filter(|(_, (_, op))| *op == data::Operation::Delete)
            .map(|(path, _)| path.clone())
            .collect();

        // 剩余路径集合
        let mut remaining_paths: HashSet<PathBuf> = all_paths
            .iter()
            .filter(|path| !to_delete.contains(*path))
            .cloned()
            .collect();

        // 递归查找和标记空目录
        let mut found_empty_dirs = true;
        while found_empty_dirs {
            found_empty_dirs = false;
            let mut new_empty_dirs = Vec::new();

            // 遍历所有剩余路径，查找空目录
            for path in remaining_paths.iter() {
                if path.is_dir() {
                    let mut is_empty = true;
                    for child in remaining_paths.iter() {
                        if child != path && child.starts_with(path) {
                            is_empty = false;
                            break;
                        }
                    }

                    if is_empty {
                        new_empty_dirs.push(path.clone());
                        found_empty_dirs = true;
                    }
                }
            }

            // 更新删除列表和剩余列表
            for empty_dir in &new_empty_dirs {
                file_info.insert(empty_dir.clone(), ("<EMPTY_DIR>".to_string(), data::Operation::Delete));
                to_delete.insert(empty_dir.clone());
                remaining_paths.remove(empty_dir);
            }
        }
    }

    // 构建最终的删除和重命名操作列表
    let mut all_delete_operations: Vec<(PathBuf, String)> = file_info
        .iter()
        .filter(|(_, (_, op))| *op == data::Operation::Delete)
        .map(|(path, (pattern, _))| (path.clone(), pattern.clone()))
        .collect();

    // 按深度优先排序删除操作
    all_delete_operations.sort_by(|(path_a, _), (path_b, _)| {
        let depth_a = path_a.components().count();
        let depth_b = path_b.components().count();
        depth_b.cmp(&depth_a)
    });

    // 执行删除操作
    if app_options.enable_deletion {
        for (file_path, pattern) in all_delete_operations {
            if app_options.verbose > 0 {
                println!("{} {:#?} <== {}", "[-]".red(), file_path, pattern);
            } else {
                println!("{} {:#?}", "[-]".red(), file_path);
            }

            if app_options.prune && file_path.exists() {
                util::remove_path(file_path)?;
            }
        }
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
