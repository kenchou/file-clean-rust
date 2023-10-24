use clap::{ArgAction, Parser};
use dirs_next as dirs;
use serde::{Deserialize, Serialize};
use serde_yaml::Error;
use std::{
    collections::HashMap,
    env,
    fs::File,
    path::{Path, PathBuf},
    process,
};
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None, disable_help_flag = true)]
struct CliOptions {
    /// Target Directory to clean
    path: Option<PathBuf>,

    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Disable delete files and directories which matched remove patterns.
    #[arg(short='D', long="disable-delete", action = ArgAction::SetFalse)]
    enable_delete: bool,

    /// Disable remove empty dir.
    #[arg(short='E', long="disable-remove-empty-dir", action = ArgAction::SetFalse)]
    prune_empty_dir: bool,

    /// Disable rename files and directories which matched patterns.
    #[arg(short='R', long="disable-rename", action = ArgAction::SetFalse)]
    enable_rename: bool,

    /// remove file if hash matched.
    #[arg(short = 'x', long = "enable-hash-match")]
    enable_hash: bool,

    /// ignored if any parents dir is .tmp
    #[arg(short = 't', long = "skip-tmp-in-parents")]
    skip_tmp: bool,

    /// Execute remove and rename action
    #[arg(long)]
    prune: bool,

    /// verbose mode
    #[arg(short, long, action = ArgAction::Count)]
    verbose: u8,

    /// Print help
    #[arg(long, action = ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug)]
struct PatternsConfig {
    remove: Vec<String>,
    remove_hash: HashMap<String, Vec<String>>,
    cleanup: Vec<String>,
}

fn main() {
    let options = CliOptions::parse();
    println!("{options:#?}"); // debug

    let config_file: Option<PathBuf>;
    let target_path = options
        .path
        .unwrap_or(PathBuf::from("."))
        .to_path_buf()
        .canonicalize()
        .expect("Failed to get absolute path");
    println!("Target Path: {target_path:#?}");

    // guess and read config
    if options.config.is_none() {
        // guess
        let mut guess_paths: Vec<_> = target_path.ancestors().map(Path::to_path_buf).collect();
        if let Some(home_dir) = dirs::home_dir() {
            guess_paths.push(home_dir);
        }
        println!("{guess_paths:#?}");
        config_file = guess_path(".cleanup-patterns.yml", guess_paths);
        println!("{config_file:#?}");
    } else {
        config_file = options.config
    }
    if config_file.is_none() {
        println!("Missing config of patterns. exit!");
        process::exit(1);
    }
    let config_file = config_file.unwrap();

    let config = load_patterns(config_file);
    println!("{config:#?}");

    for entry in WalkDir::new(target_path).into_iter().filter_map(|e| e.ok()) {
        println!("{}", entry.path().display());
    }
}

fn load_patterns(config_file: PathBuf) -> Result<PatternsConfig, Error> {
    let file = File::open(&config_file).expect("Cannot open file!");
    let config: PatternsConfig = serde_yaml::from_reader(file)?;
    Ok(config)
}

fn guess_path(test_file: &str, mut guess_paths: Vec<PathBuf>) -> Option<PathBuf> {
    if guess_paths.is_empty() {
        if let Ok(cwd) = env::current_dir() {
            guess_paths.push(cwd);
        }
        if let Some(home_dir) = dirs::home_dir() {
            guess_paths.push(home_dir);
        }
    }
    for p in dedup_vec(&guess_paths) {
        let file_path = p.join(&test_file);
        if file_path.is_file() {
            return Some(file_path);
        }
    }
    None
}

fn dedup_vec(v: &Vec<PathBuf>) -> Vec<PathBuf> {
    let mut new_vec = Vec::new();
    for i in v {
        if !new_vec.contains(i) {
            new_vec.push(i.to_path_buf());
        }
    }
    return new_vec;
}
