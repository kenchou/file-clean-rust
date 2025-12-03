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
                    // 检查是否是目录且清理结果为空（只保留路径部分，文件名为空）
                    if filepath.is_dir() {
                        let cleaned_name = PathBuf::from(&new_filename)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();

                        if cleaned_name.is_empty() {
                            // 目录名被完全清理，需要移动内容到父目录
                            return Some((
                                filepath.to_path_buf(),
                                ("".to_string(), data::Operation::MoveToParent),
                            ));
                        }
                    }

                    return Some((
                        filepath.to_path_buf(),
                        (new_filename, data::Operation::Rename),
                    ));
                }
            }

            // 检查是否为空目录（但排除符号链接目录）
            if options_ref.enable_prune_empty_dir && filepath.is_dir() && !filepath.is_symlink() {
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

            let dirs: Vec<&PathBuf> = all_paths.iter().filter(|p| p.is_dir() && !p.is_symlink()).collect();

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
                    if !to_delete.contains(&dir)
                        && dir_children.get(dir).map_or(true, |c| c.is_empty())
                    {
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
            file_info.insert(dir, ("<EMPTY_DIR>".to_string(), data::Operation::Delete));
        }
    }

    // 完成进度条
    process_bar.finish_with_message("文件处理完成");

    // 首先，直接复制所有操作到 effective_operations，不做修改
    let mut effective_operations: HashMap<PathBuf, (String, data::Operation)> = file_info.clone();

    // 构建删除路径集合，用于快速查找
    let delete_paths: HashSet<PathBuf> = effective_operations
        .iter()
        .filter(|(_, (_, op))| *op == data::Operation::Delete)
        .map(|(path, _)| path.clone())
        .collect();

    if app_options.is_debug_mode() {
        println!("删除路径集合: {:?}", delete_paths);
    }

    // 检查每个路径，如果其父目录被删除，则标记为间接删除
    let paths_to_update: Vec<(PathBuf, String)> = effective_operations
        .iter()
        .filter(|(_, (_, op))| *op != data::Operation::Delete) // 不是直接删除的项目
        .filter_map(|(path, (pattern, _op))| {
            // 检查是否有任何父目录被删除
            let mut current_path = path.clone();
            while let Some(parent) = current_path.parent() {
                let parent_pathbuf = parent.to_path_buf();
                if app_options.is_debug_mode() {
                    println!(
                        "检查路径 {:?} 的父目录 {:?} 是否在删除列表中",
                        path, parent_pathbuf
                    );
                }
                if delete_paths.contains(&parent_pathbuf) {
                    if app_options.verbose >= 2 {
                        println!(
                            "找到父目录被删除: {:?} 的父目录 {:?} 被删除",
                            path, parent_pathbuf
                        );
                    }
                    return Some((path.clone(), format!("父目录被删除: {}", pattern)));
                }
                current_path = parent_pathbuf;
            }
            None
        })
        .collect();

    // 更新受父目录删除影响的项目
    for (path, new_pattern) in paths_to_update {
        if app_options.is_debug_mode() {
            println!("更新路径: {:?} -> {}", path, new_pattern);
        }
        effective_operations.insert(path, (new_pattern, data::Operation::Delete));
    }

    // 执行删除操作
    if app_options.enable_deletion {
        // 收集所有删除操作，区分直接删除和因父目录删除而受影响的项目
        let mut direct_deletes = Vec::new();
        let mut indirect_deletes = Vec::new();

        for (file_path, (pattern, op)) in effective_operations.iter() {
            if *op == data::Operation::Delete {
                if pattern.starts_with("父目录被删除:") {
                    indirect_deletes.push((file_path, pattern));
                } else {
                    direct_deletes.push((file_path, pattern));
                }
            }
        }

        // 执行直接删除操作
        for (file_path, pattern) in direct_deletes {
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
        }

        // 显示受父目录删除影响的项目（但不执行删除，因为已经被父目录删除了）
        if !indirect_deletes.is_empty() && app_options.verbose > 0 {
            println!("{} 以下文件已随父目录删除:", "[信息]".blue());
            for (file_path, pattern) in indirect_deletes {
                let original_pattern = pattern.strip_prefix("父目录被删除: ").unwrap_or(pattern);
                println!(
                    "  {} {:#?} <== {}",
                    "[↳]".dimmed(),
                    file_path,
                    original_pattern
                );
            }
        }
    }

    // 首先处理移动到父目录的操作
    if app_options.enable_renaming {
        let move_to_parent_operations: Vec<PathBuf> = effective_operations
            .iter()
            .filter(|(_, (_, op))| *op == data::Operation::MoveToParent)
            .filter(|(_path, (pattern, _))| {
                // 过滤掉被标记为"父目录被删除"的项目
                !pattern.starts_with("父目录被删除:")
            })
            .map(|(original_path, _)| original_path.clone())
            .collect();

        for dir_path in move_to_parent_operations {
            println!("{} {:#?} ==> 移动内容到父目录", "[*]".yellow(), dir_path);

            if let Some(parent_dir) = dir_path.parent() {
                if app_options.prune {
                    // 移动目录中的所有内容到父目录
                    if let Ok(entries) = std::fs::read_dir(&dir_path) {
                        for entry in entries {
                            if let Ok(entry) = entry {
                                let source_path = entry.path();
                                let filename = entry.file_name();
                                let mut target_path = parent_dir.join(&filename);

                                // 处理命名冲突
                                if target_path.exists() {
                                    let original_name = filename.to_string_lossy();
                                    let (name_without_ext, extension) =
                                        if let Some(dot_pos) = original_name.rfind('.') {
                                            let name_part = &original_name[..dot_pos];
                                            let ext_part = &original_name[dot_pos..];
                                            (name_part, ext_part)
                                        } else {
                                            (original_name.as_ref(), "")
                                        };

                                    let mut counter = 1;
                                    loop {
                                        let new_name = format!(
                                            "{}({}){}",
                                            name_without_ext, counter, extension
                                        );
                                        target_path = parent_dir.join(&new_name);

                                        if !target_path.exists() {
                                            println!(
                                                "  {} 目标已存在，使用新名称: {}",
                                                "[提示]".blue(),
                                                new_name
                                            );
                                            break;
                                        }

                                        counter += 1;
                                        if counter > 999 {
                                            eprintln!(
                                                "{} 无法找到可用的移动目标（尝试了999个后缀）: {:?}",
                                                "[错误]".red(),
                                                source_path
                                            );
                                            break;
                                        }
                                    }
                                }

                                println!(
                                    "  --> 移动 {} 到 {}",
                                    source_path.display().to_string().cyan(),
                                    target_path.display().to_string().cyan()
                                );
                                match std::fs::rename(&source_path, &target_path) {
                                    Ok(_) => (),
                                    Err(e) => {
                                        eprintln!(
                                            "{} 移动文件失败 {:?} -> {:?}: {}",
                                            "[错误]".red(),
                                            source_path,
                                            target_path,
                                            e
                                        );
                                    }
                                }
                            }
                        }

                        // 移动完成后删除空目录
                        match std::fs::remove_dir(&dir_path) {
                            Ok(_) => println!(
                                "  --> 删除空目录 {}",
                                dir_path.display().to_string().cyan()
                            ),
                            Err(e) => {
                                eprintln!(
                                    "{} 删除空目录失败 {:?}: {}",
                                    "[错误]".red(),
                                    dir_path,
                                    e
                                );
                            }
                        }
                    } else {
                        eprintln!("{} 无法读取目录内容: {:?}", "[错误]".red(), dir_path);
                    }
                } else {
                    println!(
                        "  --> 预览：将移动目录内容到 {}",
                        parent_dir.display().to_string().cyan()
                    );
                }
            } else {
                eprintln!("{} 无法获取父目录: {:?}", "[错误]".red(), dir_path);
            }
        }
    }

    // 执行重命名操作
    if app_options.enable_renaming {
        let mut rename_operations: Vec<(PathBuf, String)> = effective_operations
            .iter()
            .filter(|(_, (_, op))| *op == data::Operation::Rename)
            .filter(|(_path, (pattern, _))| {
                // 过滤掉被标记为"父目录被删除"的项目
                !pattern.starts_with("父目录被删除:")
            })
            .map(|(original_path, (new_file_name, _))| {
                (original_path.clone(), new_file_name.clone())
            })
            .collect();

        // 按深度排序：深度大的（子项）先处理，深度小的（父项）后处理
        rename_operations.sort_by(|a, b| {
            let depth_a = a.0.components().count();
            let depth_b = b.0.components().count();
            depth_b.cmp(&depth_a) // 从深到浅排序
        });

        'outer: for (original_path, new_file_name) in rename_operations {
            println!(
                "{} {:#?} ==> {}",
                "[*]".yellow(),
                original_path,
                new_file_name
            );

            let mut final_filepath = original_path.clone();
            final_filepath.set_file_name(&new_file_name);

            // 处理重命名冲突：如果目标路径已存在，添加后缀 (1), (2), ...
            if final_filepath.exists() {
                let parent = original_path.parent().unwrap();
                let original_name = &new_file_name;

                // 分离文件名和扩展名
                let (name_without_ext, extension) = if let Some(dot_pos) = original_name.rfind('.')
                {
                    let name_part = &original_name[..dot_pos];
                    let ext_part = &original_name[dot_pos..];
                    (name_part, ext_part)
                } else {
                    (original_name.as_str(), "")
                };

                let mut counter = 1;
                loop {
                    let new_name = format!("{}({}){}", name_without_ext, counter, extension);
                    let test_path = parent.join(&new_name);

                    if !test_path.exists() {
                        println!("  {} 目标已存在，使用新名称: {}", "[提示]".blue(), new_name);
                        final_filepath = test_path;
                        break;
                    }

                    counter += 1;
                    if counter > 999 {
                        eprintln!(
                            "{} 无法找到可用的重命名目标（尝试了999个后缀）: {:?}",
                            "[错误]".red(),
                            original_path
                        );
                        continue 'outer;
                    }
                }
            }

            if app_options.prune {
                println!("--> {}", final_filepath.display().to_string().cyan());
                match rename(&original_path, &final_filepath) {
                    Ok(_) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        eprintln!(
                            "{} 源文件不存在，可能已被父目录操作影响: {:?}",
                            "[警告]".yellow(),
                            original_path
                        );
                    }
                    Err(e) => {
                        eprintln!(
                            "{} 重命名文件失败 {:?} -> {:?}: {}",
                            "[错误]".red(),
                            original_path,
                            final_filepath,
                            e
                        );
                    }
                }
            }
        }
    }

    Ok(())
}
