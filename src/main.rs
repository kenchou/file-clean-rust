use std::collections::{HashMap, HashSet};
use std::fs::rename;
use std::path::PathBuf;
use std::sync::Arc;

use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
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

    println!("正在扫描文件...");
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    spinner.set_message("scanning files...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    // 仅扫描一次文件系统，收集所有路径
    let mut file_count = 0;
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
        .filter_map(|e| {
            if let Ok(_) = &e {
                file_count += 1;
                if file_count % 1000 == 0 {
                    spinner.set_message(format!("已扫描 {} 个文件...", file_count));
                }
            }
            e.ok()
        })
        .collect();
    spinner.finish_with_message(format!("扫描完成，共 {} 个文件", file_count));

    // 并行处理文件信息
    let options_ref = &app_options;
    let matcher_ref = &pattern_matcher;

    println!("正在处理文件...");
    let process_bar = ProgressBar::new(entries.len() as u64);
    process_bar.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len}\n{msg}",
            )
            .unwrap()
            .progress_chars("█▓▒░ "),
    );

    let file_info_results: Vec<_> = entries
        .par_iter()
        .filter_map(|entry| {
            let filepath = entry.path();
            // 更新进度条
            process_bar.inc(1);
            // 显示当前处理的文件名
            if let Some(name) = filepath.file_name().and_then(|n| n.to_str()) {
                if process_bar.position() % 100 == 0 {
                    process_bar.set_message(format!("处理: {}", name));
                }
            }

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

    // 处理递归的空目录删除 - 优化版本
    if app_options.enable_deletion && app_options.enable_prune_empty_dir {
        process_bar.set_message("空目录检测中...".to_string());

        // 第一阶段：收集需要删除的目录
        let dirs_to_mark_delete = {
            // 创建局部作用域，确保借用在此结束
            let paths_set: HashSet<&PathBuf> = all_paths.iter().collect();
            let mut to_delete: HashSet<&PathBuf> = file_info
                .iter()
                .filter(|(_, (_, op))| *op == data::Operation::Delete)
                .map(|(path, _)| path)
                .collect();

            // 目录子项映射
            let mut dir_children: HashMap<&PathBuf, Vec<&PathBuf>> = HashMap::new();

            let dirs: Vec<&PathBuf> = all_paths.iter()
                .filter(|p| p.is_dir())
                .collect();

            // 初始化目录映射
            for &dir in &dirs {
                dir_children.insert(dir, Vec::new());
            }

            // 构建父子关系
            for path in all_paths.iter() {
                if let Some(parent) = path.parent().map(PathBuf::from) {
                    if let Some(actual_parent) = paths_set.get(&parent) {
                        if !to_delete.contains(path) {
                            dir_children.entry(actual_parent).or_default().push(path);
                        }
                    }
                }
            }

            // 识别所有空目录
            let mut empty_dirs_result = Vec::new();
            let mut empty_dirs = Vec::with_capacity(dirs.len() / 2);

            for _ in 0..dirs.len() {
                empty_dirs.clear();

                for &dir in dirs.iter() {
                    if !to_delete.contains(&dir) &&
                        dir_children.get(dir).map_or(true, |c| c.is_empty()) {
                        empty_dirs.push(dir);
                    }
                }

                if empty_dirs.is_empty() {
                    break;
                }

                for &dir in &empty_dirs {
                    empty_dirs_result.push(dir.clone());
                    to_delete.insert(dir);

                    // 更新父目录的子列表
                    if let Some(parent_buf) = dir.parent().map(PathBuf::from) {
                        if let Some(parent) = paths_set.get(&parent_buf) {
                            if let Some(children) = dir_children.get_mut(parent) {
                                children.retain(|&p| p != dir);
                            }
                        }
                    }
                }
            }

            empty_dirs_result
        }; // to_delete 的生命周期在这里结束

        // 第二阶段：更新 file_info
        for dir in dirs_to_mark_delete {
            file_info.insert(
                dir,
                ("<EMPTY_DIR>".to_string(), data::Operation::Delete),
            );
        }
    }

    // 完成进度条
    process_bar.finish_with_message("文件处理完成");

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
