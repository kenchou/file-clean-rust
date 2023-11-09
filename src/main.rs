mod fnmatch_regex;
use clap::{ArgAction, Parser};
use colored::*;
use dirs_next as dirs;
use fancy_regex::Regex;
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::{
    collections::HashMap,
    env,
    fs::{remove_dir_all, remove_file, rename, File},
    io::{self, Read},
    path::{Path, PathBuf},
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

    /// Enable delete files and directories which matched remove patterns.
    #[arg(short = 'd', long = "enable-deletion", default_value = "true")]
    enable_deletion: bool,
    /// Disable delete files and directories which matched remove patterns.
    #[arg(short='D', long="disable-deletion", action = ArgAction::SetTrue, conflicts_with = "enable_deletion")]
    disable_deletion: bool,

    /// Enable hash matching delete feature.
    #[arg(short = 'x', long = "enable-hash-match", default_value = "true")]
    enable_hash_matching: bool,
    /// Disable hash matching delete feature.
    #[arg(short = 'X', long = "disable-hash-match", action = ArgAction::SetTrue, conflicts_with = "enable_hash_matching")]
    disble_hash_matching: bool,

    /// Ensable remove empty dir.
    #[arg(short = 'e', long = "enable-remove-empty-dir", default_value = "true")]
    enable_prune_empty_dir: bool,
    /// Disable remove empty dir.
    #[arg(short='E', long="disable-remove-empty-dir", action = ArgAction::SetTrue, conflicts_with = "enable_prune_empty_dir")]
    disable_prune_empty_dir: bool,

    /// Enable rename files and directories which matched patterns.
    #[arg(short = 'r', long = "enable-renaming", default_value = "true")]
    enable_renaming: bool,
    /// Disable rename files and directories which matched patterns.
    #[arg(short='R', long="disable-renaming", action = ArgAction::SetTrue, conflicts_with = "enable_renaming")]
    disable_renaming: bool,

    /// ignored if any parents dir is .tmp
    #[arg(short = 't', long = "skip-tmp-dirs", default_value = "true")]
    skip_tmp: bool,
    /// ignored if any parents dir is .tmp
    #[arg(short = 'T', long = "no-skip-tmp-dirs", action = ArgAction::SetTrue, conflicts_with = "skip_tmp")]
    no_skip_tmp: bool,

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

#[derive(Debug)]
struct AppOptions {
    enable_deletion: bool,
    enable_hash_matching: bool,
    // enable_prune_empty_dir: bool,
    enable_renaming: bool,
    // skip_tmp: bool,
    prune: bool,
    // verbose: u8,
    config_file: PathBuf,
    target_path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug)]
struct PatternsConfig {
    remove: Vec<String>,
    remove_hash: HashMap<String, Vec<String>>,
    cleanup: Vec<String>,
}

impl PatternsConfig {
    fn from_config_file(config_file: &Path) -> PatternsConfig {
        let file = File::open(&config_file).expect("Cannot open file!");
        let values: HashMap<String, Value> = serde_yaml::from_reader(file).unwrap();
        let mut config = PatternsConfig {
            remove: vec![],
            remove_hash: HashMap::new(),
            cleanup: vec![],
        };
        for (key, value) in values {
            match key.as_str() {
                "remove" => match value {
                    Value::String(s) => config
                        .remove
                        .extend(s.lines().map(|v| v.trim().to_string()).collect::<Vec<_>>()),
                    Value::Sequence(s) => config.remove.extend(
                        s.iter()
                            .map(|v| v.as_str().unwrap().to_string())
                            .collect::<Vec<_>>(),
                    ),
                    _ => {}
                },
                "remove_hash" => match value {
                    Value::Mapping(map) => config.remove_hash.extend(
                        map.iter()
                            .map(|(k, v)| {
                                (
                                    k.as_str().unwrap().to_string(),
                                    match v {
                                        Value::Sequence(hash_list) => hash_list
                                            .into_iter()
                                            .map(|vv| vv.as_str().unwrap().to_string())
                                            .collect(),
                                        _ => vec![],
                                    },
                                )
                            })
                            .collect::<Vec<_>>(),
                    ),
                    _ => {}
                },
                "cleanup" => match value {
                    Value::String(s) => config
                        .cleanup
                        .extend(s.lines().map(|v| v.trim().to_string()).collect::<Vec<_>>()),
                    Value::Sequence(s) => config.cleanup.extend(
                        s.iter()
                            .map(|v| v.as_str().unwrap().to_string())
                            .collect::<Vec<_>>(),
                    ),
                    _ => {}
                },
                _ => {}
            }
        }
        config
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
        let config = PatternsConfig::from_config_file(config_file);
        let patterns_to_remove =
            create_mixed_regex_list(config.remove.iter().map(AsRef::as_ref).collect()).unwrap();
        let patterns_to_rename =
            create_regex_list(config.cleanup.iter().map(AsRef::as_ref).collect()).unwrap();
        let patterns_to_remove_with_hash = create_patterns_with_hash(config.remove_hash).unwrap();
        Ok(PatternMacher {
            patterns_to_remove,
            patterns_to_remove_with_hash,
            patterns_to_rename,
        })
    }

    fn match_remove_pattern(&self, test_file: &str) -> (bool, Option<String>) {
        for re in &self.patterns_to_remove {
            if re.is_match(test_file).unwrap() {
                return (true, Some(re.to_string()));
            }
        }
        return (false, None);
    }

    fn match_remove_hash(&self, test_file: &str) -> (bool, Option<String>) {
        for (re, hash_list) in &self.patterns_to_remove_with_hash {
            if re.is_match(test_file).unwrap() {
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
            let pattern = pattern.trim();
            // println!(">>> {:#?}", pattern);
            if pattern.starts_with("/") {
                Regex::new(&pattern[1..]).unwrap()
            } else {
                Regex::new(fnmatch_regex::glob_to_regex_string(pattern).as_str()).unwrap()
            }
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
        .map(|pattern| {
            // println!("---> {:#?}", pattern);
            Regex::new(pattern.trim()).unwrap()
        })
        .collect();
    Ok(regexes)
}

fn create_patterns_with_hash(
    patterns: HashMap<String, Vec<String>>,
) -> Result<Vec<(Regex, Vec<String>)>, Box<dyn std::error::Error>> {
    let patterns_to_remove_with_hash = patterns
        .into_iter()
        .map(|(key, value)| {
            // println!("hash --> {}", key);
            (
                Regex::new(fnmatch_regex::glob_to_regex_string(&key).as_str()).unwrap(),
                value,
            )
        })
        .collect();
    Ok(patterns_to_remove_with_hash)
}

fn get_guess_paths(target_path: &PathBuf) -> Vec<PathBuf> {
    let mut guess_paths: Vec<_> = target_path.ancestors().map(Path::to_path_buf).collect();
    if let Some(home_dir) = dirs::home_dir() {
        guess_paths.push(home_dir);
    }
    guess_paths
}

fn main() -> std::io::Result<()> {
    let app_options: AppOptions;
    {
        // init AppOptions
        let options = CliOptions::parse();

        let target_path = options
            .path
            .clone()
            .unwrap_or(PathBuf::from("."))
            .to_path_buf()
            .canonicalize()?;

        app_options = AppOptions {
            enable_deletion: if options.disable_deletion {
                false
            } else {
                options.enable_deletion
            },
            enable_hash_matching: if options.disble_hash_matching {
                false
            } else {
                options.enable_hash_matching
            },
            // enable_prune_empty_dir: if options.disable_prune_empty_dir {
            //     false
            // } else {
            //     options.enable_prune_empty_dir
            // },
            enable_renaming: if options.disable_renaming {
                false
            } else {
                options.enable_renaming
            },
            // skip_tmp: if options.no_skip_tmp {
            //     false
            // } else {
            //     options.skip_tmp
            // },
            prune: options.prune,
            // verbose: options.verbose,
            config_file: match options.config {
                None => guess_path(".cleanup-patterns.yml", get_guess_paths(&target_path)).unwrap(),
                Some(p) => p,
            },
            target_path,
        };
    }
    println!("{app_options:#?}"); // debug

    let config_file = app_options.config_file;

    let pattern_matcher = PatternMacher::from_config_file(&config_file).unwrap();
    // println!("{pattern_matcher:#?}");

    let mut pending_remove: Vec<(PathBuf, String)> = vec![];
    let mut pending_rename: Vec<(PathBuf, String)> = vec![];
    for entry in WalkDir::new(app_options.target_path)
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

        if app_options.enable_deletion {
            let (mut matched, mut pattern) = pattern_matcher.match_remove_pattern(filename);
            if matched {
                let p = pattern.unwrap();
                println!(" <== {}", p);
                pending_remove.push((filepath.to_path_buf(), p));
                continue;
            } else if app_options.enable_hash_matching {
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

        if app_options.enable_renaming {
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

    if app_options.enable_deletion {
        for (file_path, pattern) in pending_remove {
            println!("{} {:#?} <== {}", "[-]".red(), file_path, pattern);
            if app_options.prune {
                remove_path(file_path)?;
            }
        }
    }

    if app_options.enable_renaming {
        for (file_path, new_file_name) in pending_rename {
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

fn is_not_hidden(entry: &DirEntry) -> bool {
    entry.file_name().to_string_lossy() != ".tmp"
        && entry.path().parent().map_or(true, |p| {
            p.file_name()
                .map_or(true, |p| p.to_string_lossy() != ".tmp")
        })
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
