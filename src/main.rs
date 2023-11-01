use clap::{ArgAction, Parser};
use colored::*;
use dirs_next as dirs;
use fnmatch_regex;
use md5::{Digest, Md5};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    fs::{remove_dir_all, remove_file, rename, File},
    io::{self, Read},
    path::{Path, PathBuf},
    process,
};
use walkdir::{DirEntry, WalkDir};

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

impl PatternsConfig {
    fn from_config_file(config_file: &Path) -> Result<PatternsConfig, serde_yaml::Error> {
        let file = File::open(&config_file).expect("Cannot open file!");
        let config: PatternsConfig = serde_yaml::from_reader(file)?;
        return Ok(config);
    }
}

#[derive(Debug)]
struct PatternMacher {
    patterns_to_remove: Vec<Regex>,
    patterns_to_remove_with_hash: Vec<(Regex, Vec<String>)>,
    patterns_to_rename: Vec<Regex>,
}

impl PatternMacher {
    fn from_config_file(config_file: &Path) -> Result<PatternMacher, serde_yaml::Error> {
        let config = PatternsConfig::from_config_file(config_file).unwrap();
        let patterns_to_remove =
            create_mixed_regex_list(config.remove.iter().map(|s| s.as_str()).collect()).unwrap();
        let patterns_to_rename =
            create_regex_list(config.cleanup.iter().map(|s| s.as_str()).collect()).unwrap();
        let patterns_to_remove_with_hash = create_patterns_with_hash(config.remove_hash).unwrap();
        Ok(PatternMacher {
            patterns_to_remove,
            patterns_to_remove_with_hash,
            patterns_to_rename,
        })
    }

    fn match_remove_pattern(&self, test_file: &str) -> (bool, Option<String>) {
        for re in &self.patterns_to_remove {
            if re.is_match(test_file) {
                return (true, Some(re.to_string()));
            }
        }
        return (false, None);
    }

    fn match_remove_hash(&self, test_file: &str) -> (bool, Option<String>) {
        for (re, hash_list) in &self.patterns_to_remove_with_hash {
            if re.is_match(test_file) {
                let mut file = File::open(test_file).unwrap();
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer).unwrap();
                let mut hasher = Md5::new();
                hasher.update(&buffer);

                let hash = format!("{:x}", hasher.finalize());
                if hash_list.contains(&hash) {
                    println!(" <== {}:{}", re.to_string(), hash);
                    return (true, Some(format!("{}:{}", re.to_string(), hash)));
                }
            }
        }
        return (false, None);
    }

    fn clean_filename(&self, filename: &str) -> String {
        let mut new_filename = PathBuf::from(filename.to_string())
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        for re in &self.patterns_to_rename {
            new_filename = re.replace_all(&new_filename, "").to_string();
        }
        let mut full_path = PathBuf::from(filename.to_string());
        full_path.set_file_name(new_filename);
        let new_filename = full_path.to_str().unwrap().to_string();
        return new_filename;
    }
}

/**
 * 创建正则表达式列表，通配符形式转为正则表达式
 */
fn create_mixed_regex_list(patterns: Vec<&str>) -> Result<Vec<Regex>, Box<dyn std::error::Error>> {
    let regexes: Vec<Regex> = patterns
        .iter()
        .map(|pattern| {
            let re: Regex;
            if pattern.starts_with("/") {
                re = Regex::new(&pattern[1..]).unwrap()
            } else {
                re = fnmatch_regex::glob_to_regex(pattern).unwrap()
            }
            re
        })
        .collect();

    Ok(regexes)
}

/**
 * 创建正则表达式列表
 */
fn create_regex_list(patterns: Vec<&str>) -> Result<Vec<Regex>, Box<dyn std::error::Error>> {
    let regexes: Vec<Regex> = patterns
        .iter()
        .map(|pattern| Regex::new(&pattern).unwrap())
        .collect();

    Ok(regexes)
}

fn create_patterns_with_hash(
    patterns: HashMap<String, Vec<String>>,
) -> Result<Vec<(Regex, Vec<String>)>, Box<dyn std::error::Error>> {
    let patterns_to_remove_with_hash = patterns
        .into_iter()
        .map(|(key, value)| (Regex::new(&key).unwrap(), value))
        .collect();
    Ok(patterns_to_remove_with_hash)
}

fn main() -> std::io::Result<()> {
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
        // println!("{guess_paths:#?}");
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

    let pattern_matcher = PatternMacher::from_config_file(&config_file).unwrap();
    // println!("{pattern_matcher:#?}");

    let mut pending_remove: Vec<(PathBuf, String)> = vec![];
    let mut pending_rename: Vec<(PathBuf, String)> = vec![];
    for entry in WalkDir::new(target_path)
        .into_iter()
        .filter_entry(|e| is_not_hidden(e))
        .filter_map(|e| e.ok())
    {
        let filepath = entry.path();
        let filename = entry.file_name().to_str().unwrap();
        let depth = entry.depth();
        let prefix = " ".repeat(depth * 4);

        // print!("{filename:#?}");
        // print!("{}{}", prefix, name.display());
        print!("{}├── {}", prefix, filename);

        if options.enable_delete {
            let (mut matched, mut pattern) = pattern_matcher.match_remove_pattern(filename);
            if matched {
                let p = pattern.unwrap();
                println!(" <== {}", p);
                pending_remove.push((filepath.to_path_buf(), p));
                continue;
            } else {
                // test filename and hash
                (matched, pattern) = pattern_matcher.match_remove_hash(filepath.to_str().unwrap());
                if matched {
                    let p = pattern.unwrap();
                    println!(" <== {}", p);
                    pending_remove.push((filepath.to_path_buf(), p));
                    continue;
                }
            }
        }

        if options.enable_rename {
            let new_filename = pattern_matcher.clean_filename(filename);
            if new_filename != filename {
                println!(" ==> {new_filename:#?}");
                pending_rename.push((filepath.to_path_buf(), new_filename));
                continue;
            }
        }
        println!();
    }
    println!("files to delete: {pending_remove:#?}");
    println!("files to rename: {pending_rename:#?}");

    if options.enable_delete {
        for (file_path, pattern) in pending_remove {
            println!("{} {:#?} <== {}", "[-]".red(), file_path, pattern);
            remove_path(file_path)?;
        }
    }

    if options.enable_rename {
        for (file_path, new_file_path) in pending_rename {
            println!("{} {:#?} ==> {}", "[*]".yellow(), file_path, new_file_path);
            rename(file_path, new_file_path)?;
        }
    }
    Ok(())
}

fn is_not_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| !s.starts_with('.'))
        .unwrap_or(false)
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

fn remove_path(path: PathBuf) -> io::Result<()> {
    match remove_file(&path) {
        Ok(()) => Ok(()),
        Err(_) => remove_dir_all(path),
    }
}
