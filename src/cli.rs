use std::path::PathBuf;
use std::env;

use clap::{arg, command, value_parser, ArgAction};

use crate::data;
use crate::util;

pub fn parse() -> std::result::Result<data::AppOptions, std::io::Error> {
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

    Ok(data::AppOptions {
        enable_deletion: matches.get_flag("delete") || !matches.get_flag("no-delete"),
        enable_hash_matching: matches.get_flag("hash") || !matches.get_flag("no-hash"),
        enable_prune_empty_dir: matches.get_flag("remove-empty-dir")
            || !matches.get_flag("no-remove-empty-dir"),
        enable_renaming: matches.get_flag("rename") || !matches.get_flag("no-rename"),
        skip_parent_tmp: matches.get_flag("skip-tmp") || !matches.get_flag("no-skip-tmp"),
        prune: matches.get_flag("prune"),
        verbose: matches.get_count("verbose"),
        config_file: match matches.get_one::<PathBuf>("config") {
            None => util::guess_path(".cleanup-patterns.yml", util::get_guess_paths(&target_path))
                .unwrap(),
            Some(p) => p.clone(),
        },
        target_path,
    })
}
//EOP
