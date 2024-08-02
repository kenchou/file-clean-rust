use std::fs::rename;
use std::path::PathBuf;

use colored::*;
use walkdir::WalkDir;

mod data;
mod fnmatch_regex;
mod p2tree;
mod tprint;
mod pmatcher;
mod pconfig;
mod cli;
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
                operation_list.push((filepath.to_path_buf(), new_filename, data::Operation::Rename));
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

        operation_list.push((filepath.to_path_buf(), "".to_string(), data::Operation::None));
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
    operation_list.retain(|(_, _, op)| match op {
        data::Operation::None => false,
        _ => true,
    });
    // Sort the operation list in depth-first order.
    operation_list.sort_by(|a, b| {
        let depth_a = a.0.components().count();
        let depth_b = b.0.components().count();
        depth_b.cmp(&depth_a)
    });
    // execute
    if app_options.enable_deletion {
        for (file_path, pattern, _) in operation_list
            .iter()
            .filter(|(_, _, op)| *op == data::Operation::Delete)
        {
            if app_options.verbose > 0 {
                println!("{} {:#?} <== {}", "[-]".red(), file_path, pattern);
            } else {
                println!("{} {:#?}", "[-]".red(), file_path);
            }

            if app_options.prune && file_path.exists() {
                util::remove_path(file_path.clone())?;
            }
        }
    }

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
