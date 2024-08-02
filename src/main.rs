use std::env;
use std::fs::rename;
use std::path::PathBuf;

use clap::{arg, command, value_parser, ArgAction};
use colored::*;
use walkdir::WalkDir;

mod data;
mod fnmatch_regex;
mod p2tree;
mod tprint;
mod pmatcher;
mod pconfig;
mod util;

fn main() -> std::io::Result<()> {
    let app_options: data::AppOptions;
    // init AppOptions
    {
        let app = command!() // requires `cargo` feature
            .arg(arg!([path] "target path to clean up").value_parser(value_parser!(PathBuf)))
            .arg(
                arg!(-c --config <FILE> "Sets a custom config file")
                    // We don't have syntax yet for optional options, so manually calling `required`
                    .required(false)
                    .value_parser(value_parser!(PathBuf)),
            )
            .arg(
                arg!(
                    -d --delete ... "Match filename deletion rule. [default]"
                )
                .action(ArgAction::SetTrue), // .hide(true),
            )
            .arg(
                arg!(-D --"no-delete" ... "Do not match filename deletion rule.")
                    .value_parser(value_parser!(bool))
                    .action(ArgAction::SetTrue)
                    .conflicts_with("delete"),
            )
            .arg(
                arg!(
                    -x --hash ... "Match hash deletion rule. [default]"
                )
                .action(ArgAction::SetTrue), // .hide(true),
            )
            .arg(
                arg!(
                    -X --"no-hash" ... "Do not match hash deletion rule."
                )
                .action(ArgAction::SetTrue)
                .conflicts_with("hash"),
            )
            .arg(
                arg!(
                    -r --rename ... "Match file renaming rule. [default]"
                )
                .action(ArgAction::SetTrue), // .hide(true),
            )
            .arg(
                arg!(
                    -R --"no-rename" ... "Do not match file renaming rule."
                )
                .action(ArgAction::SetTrue)
                .conflicts_with("rename"),
            )
            .arg(
                arg!(
                    -t --"skip-tmp" ... "Skip the .tmp directory. [default]"
                )
                .action(ArgAction::SetTrue), // .hide(true),
            )
            .arg(
                arg!(
                    -T --"no-skip-tmp" ... "Do not skip the .tmp directory."
                )
                .action(ArgAction::SetTrue)
                .conflicts_with("skip-tmp"),
            )
            .arg(
                arg!(
                    -e --"remove-empty-dir" ... "Delete empty directories. [default]"
                )
                .action(ArgAction::SetTrue), // .hide(true),
            )
            .arg(
                arg!(
                    -E --"no-remove-empty-dir" ... "Do not delete empty directories."
                )
                .action(ArgAction::SetTrue)
                .conflicts_with("remove-empty-dir"),
            )
            .arg(arg!(--prune ... "Perform the prune action.").action(ArgAction::SetTrue))
            .arg(arg!(
                -v --verbose ... "Verbose mode."
            ));

        let matches = app.get_matches();
        let target_path = matches
            .get_one::<PathBuf>("path")
            .unwrap_or(&PathBuf::from("."))
            .to_path_buf()
            .canonicalize()?;

        app_options = data::AppOptions {
            enable_deletion: matches.get_flag("delete") || !matches.get_flag("no-delete"),
            enable_hash_matching: matches.get_flag("hash") || !matches.get_flag("no-hash"),
            enable_prune_empty_dir: matches.get_flag("remove-empty-dir")
                || !matches.get_flag("no-remove-empty-dir"),
            enable_renaming: matches.get_flag("rename") || !matches.get_flag("no-rename"),
            skip_parent_tmp: matches.get_flag("skip-tmp") || !matches.get_flag("no-skip-tmp"),
            prune: matches.get_flag("prune"),
            verbose: matches.get_count("verbose"),
            config_file: match matches.get_one::<PathBuf>("config") {
                None => util::guess_path(".cleanup-patterns.yml", util::get_guess_paths(&target_path)).unwrap(),
                Some(p) => p.clone(),
            },
            target_path,
        };
    }

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
