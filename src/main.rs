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

    let mut operation_list: Vec<(PathBuf, String, data::Operation)> = vec![]; // Path, Pattern, Operation
    for entry in WalkDir::new(&app_options.target_path)
        // .contents_first(true)
        .sort_by(|a, b| {
            a.file_type()
                .is_dir()
                .cmp(&b.file_type().is_dir())
                .reverse()
                .then(a.file_name().cmp(b.file_name()))
        })
        .into_iter()
        .filter_entry(|e| !app_options.skip_parent_tmp || util::is_not_hidden(e))
        .filter_map(|e| e.ok())
    {
        let filepath = entry.path();
        let filename = entry.file_name().to_str().unwrap();

        if app_options.enable_deletion {
            let (mut matched, mut pattern) = pattern_matcher.match_remove_pattern(filename);
            if matched {
                let p = pattern.unwrap();
                operation_list.push((filepath.to_path_buf(), p, data::Operation::Delete));
                continue;
            } else if app_options.enable_hash_matching {
                // test filename and hash
                (matched, pattern) = pattern_matcher.match_remove_hash(filepath.to_str().unwrap());
                if matched {
                    let p = pattern.unwrap();
                    operation_list.push((filepath.to_path_buf(), p, data::Operation::Delete));
                    continue;
                }
            }
        }

        if app_options.enable_renaming {
            let new_filename = pattern_matcher.clean_filename(filename);
            if new_filename != filename {
                operation_list.push((
                    filepath.to_path_buf(),
                    new_filename,
                    data::Operation::Rename,
                ));
                continue;
            }
        }

        if app_options.enable_prune_empty_dir
            && filepath.is_dir()
            && filepath.read_dir()?.next().is_none()
        {
            operation_list.push((
                filepath.to_path_buf(),
                "<EMPTY_DIR>".to_string(),
                data::Operation::Delete,
            ))
        }

        operation_list.push((
            filepath.to_path_buf(),
            "".to_string(),
            data::Operation::None,
        ));
    }

    if app_options.is_debug_mode() {
        println!("* operation_list: {:#?}", operation_list);
    }

    // dir tree
    if app_options.verbose >= 2 {
        tprint::print_tree(p2tree::path_list_to_tree(
            &operation_list,
            &app_options.target_path,
        ));
    }

    // Remove the entries that don't require operation.
    operation_list.retain(|(_, _, op)| !matches!(op, data::Operation::None));

    // 创建所有操作的列表
    let mut all_delete_operations: Vec<(PathBuf, String)> = Vec::new();

    // 添加初始的删除操作
    for (file_path, pattern, op) in operation_list.iter() {
        if *op == data::Operation::Delete {
            all_delete_operations.push((file_path.clone(), pattern.clone()));
        }
    }

    // 如果启用了空目录清理，模拟删除过程找出所有会变空的目录
    if app_options.enable_deletion && app_options.enable_prune_empty_dir {
        // 创建当前文件系统状态的副本以进行模拟
        let mut remaining_paths = std::collections::HashSet::new();
        for entry in WalkDir::new(&app_options.target_path)
            .into_iter()
            .filter_entry(|e| !app_options.skip_parent_tmp || util::is_not_hidden(e))
            .filter_map(|e| e.ok())
        {
            remaining_paths.insert(entry.path().to_path_buf());
        }

        // 从集合中移除所有已标记为删除的文件和目录
        for (path, _) in &all_delete_operations {
            remaining_paths.remove(path);
        }

        // 反复检查并"删除"空目录，直到没有新的空目录
        let mut found_empty_dirs = true;
        while found_empty_dirs {
            found_empty_dirs = false;
            let mut new_empty_dirs = Vec::new();

            // 按深度降序排列的路径（先处理最深的目录）
            let mut sorted_paths: Vec<PathBuf> = remaining_paths.iter().cloned().collect();
            sorted_paths.sort_by(|a, b| {
                let depth_a = a.components().count();
                let depth_b = b.components().count();
                depth_b.cmp(&depth_a)
            });

            // 查找新的空目录
            for path in sorted_paths.iter() {
                if path.is_dir() {
                    let mut is_empty = true;
                    // 检查这个目录是否为空（没有子项）
                    for child in sorted_paths.iter() {
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

            // 将新发现的空目录添加到删除列表
            for empty_dir in &new_empty_dirs {
                all_delete_operations.push((empty_dir.clone(), "<EMPTY_DIR>".to_string()));
                remaining_paths.remove(empty_dir);
            }
        }
    }

    // 执行删除操作
    if app_options.enable_deletion {
        // 按深度优先顺序排序删除操作（先删除最深的路径）
        all_delete_operations.sort_by(|(path_a, _), (path_b, _)| {
            let depth_a = path_a.components().count();
            let depth_b = path_b.components().count();
            depth_b.cmp(&depth_a)
        });

        // 输出并执行所有删除操作
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
        for (file_path, new_file_name, _) in operation_list
            .iter()
            .filter(|(_, _, op)| *op == data::Operation::Rename)
        {
            println!("{} {:#?} ==> {}", "[*]".yellow(), file_path, new_file_name);
            let mut new_filepath = file_path.clone();
            new_filepath.set_file_name(new_file_name);
            if app_options.prune {
                println!("--> {}", new_filepath.display().to_string().cyan());
                rename(file_path, new_filepath)?;
            }
        }
    }

    Ok(())
}
//EOP
