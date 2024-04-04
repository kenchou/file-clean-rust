use clap::{arg, command, value_parser, ArgAction};
use colored::*;
use dirs_next as dirs;
use fancy_regex::Regex;
use md5::{Digest, Md5};
use slab_tree::{NodeId, NodeRef, Tree, TreeBuilder};
use std::collections::HashMap;
use std::env;
use std::fs::{read_link, remove_dir_all, remove_file, rename, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

mod fnmatch_regex;

const GLYPH_TREE_SPACE: &str = "    ";
const GLYPH_TREE_BRANCH: &str = "│   ";
const GLYPH_TEE: &str = "├── ";
const GLYPH_LAST: &str = "└── ";

const SYMBOL_ROOT: &str = "📂";
const SYMBOL_DIR: &str = "📁";
const SYMBOL_FILE: &str = "📄";
const SYMBOL_LINK: &str = "🔗";
const SYMBOL_BROKEN_ARROW: &str = "!>"; // ↛ ⥇ ⓧ ⊗ ⊘ ⤍ ⤑
const SYMBOL_LINK_ARROW: &str = "->";
const SYMBOL_DELETE: &str = "[-]"; // ␡
const SYMBOL_RENAME: &str = "[*]"; //

#[derive(Debug, PartialEq)]
enum Operation {
    None,
    Delete,
    Rename,
}

#[derive(Debug)]
struct AppOptions {
    enable_deletion: bool,
    enable_hash_matching: bool,
    enable_renaming: bool,
    enable_prune_empty_dir: bool,
    skip_parent_tmp: bool,
    prune: bool,
    verbose: u8,
    config_file: PathBuf,
    target_path: PathBuf,
}

impl AppOptions {
    fn is_debug_mode(&self) -> bool {
        self.verbose >= 3
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct PatternsConfig {
    remove: Vec<String>,
    remove_hash: HashMap<String, Vec<String>>,
    cleanup: Vec<String>,
}

impl PatternsConfig {
    fn from_config_file(config_file: &Path) -> PatternsConfig {
        let file = File::open(config_file).expect("Cannot open file!");
        let values: HashMap<String, serde_yaml::Value> = serde_yaml::from_reader(file).unwrap();
        let mut config = PatternsConfig {
            remove: vec![],
            remove_hash: HashMap::new(),
            cleanup: vec![],
        };
        for (key, value) in values {
            match key.as_str() {
                "remove" => match value {
                    serde_yaml::Value::String(s) => config
                        .remove
                        .extend(s.lines().map(|v| v.trim().to_string()).collect::<Vec<_>>()),
                    serde_yaml::Value::Sequence(s) => config.remove.extend(
                        s.iter()
                            .map(|v| v.as_str().unwrap().to_string())
                            .collect::<Vec<_>>(),
                    ),
                    _ => {}
                },
                "remove_hash" => {
                    if let serde_yaml::Value::Mapping(map) = value {
                        config.remove_hash.extend(
                            map.iter()
                                .map(|(k, v)| {
                                    (
                                        k.as_str().unwrap().to_string(),
                                        match v {
                                            serde_yaml::Value::Sequence(hash_list) => hash_list
                                                .iter()
                                                .map(|vv| vv.as_str().unwrap().to_string())
                                                .collect(),
                                            _ => vec![],
                                        },
                                    )
                                })
                                .collect::<Vec<_>>(),
                        )
                    }
                }
                "cleanup" => match value {
                    serde_yaml::Value::String(s) => config
                        .cleanup
                        .extend(s.lines().map(|v| v.trim().to_string()).collect::<Vec<_>>()),
                    serde_yaml::Value::Sequence(s) => config.cleanup.extend(
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
struct PatternMatcher {
    patterns_to_remove: Vec<Regex>,
    patterns_to_remove_with_hash: Vec<(Regex, Vec<String>)>,
    patterns_to_rename: Vec<Regex>,
}

impl PatternMatcher {
    fn from_config_file(config_file: &Path) -> PatternMatcher {
        let config = PatternsConfig::from_config_file(config_file);
        let patterns_to_remove =
            create_mixed_regex_list(config.remove.iter().map(AsRef::as_ref).collect());
        let patterns_to_rename =
            create_regex_list(config.cleanup.iter().map(AsRef::as_ref).collect());
        let patterns_to_remove_with_hash = create_patterns_with_hash(config.remove_hash);
        PatternMatcher {
            patterns_to_remove,
            patterns_to_remove_with_hash,
            patterns_to_rename,
        }
    }

    fn match_remove_pattern(&self, test_file: &str) -> (bool, Option<String>) {
        for re in &self.patterns_to_remove {
            if re.is_match(test_file).unwrap() {
                return (true, Some(re.to_string()));
            }
        }
        (false, None) // return
    }

    fn match_remove_hash(&self, test_file: &str) -> (bool, Option<String>) {
        let filename = Path::new(test_file).file_name().unwrap().to_str().unwrap();
        for (re, hash_list) in &self.patterns_to_remove_with_hash {
            if re.is_match(filename).unwrap() {
                let mut file = File::open(test_file).unwrap();
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer).unwrap();
                let mut hash_calculator = Md5::new();
                hash_calculator.update(&buffer);

                let hash = format!("{:x}", hash_calculator.finalize());
                if hash_list.contains(&hash) {
                    return (true, Some(format!("{}:{}", re, hash)));
                }
            }
        }
        (false, None) // return
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
        new_filename // return new_filename
    }
}

/**
 * 创建正则表达式列表，通配符形式转为正则表达式
 */
fn create_mixed_regex_list(patterns: Vec<&str>) -> Vec<Regex> {
    patterns
        .iter()
        .map(|pattern| {
            let pattern = pattern.trim();
            // println!(">>> {:#?}", pattern);
            if let Some(stripped) = pattern.strip_prefix('/') {
                Regex::new(stripped).unwrap()
            } else {
                Regex::new(fnmatch_regex::glob_to_regex_string(pattern).as_str()).unwrap()
            }
        })
        .collect()
}

/**
 * 创建正则表达式列表
 */
fn create_regex_list(patterns: Vec<&str>) -> Vec<Regex> {
    patterns
        .iter()
        .map(|pattern| {
            // println!("---> {:#?}", pattern);
            Regex::new(pattern.trim()).unwrap()
        })
        .collect()
}

fn create_patterns_with_hash(patterns: HashMap<String, Vec<String>>) -> Vec<(Regex, Vec<String>)> {
    patterns
        .into_iter()
        .map(|(key, value)| {
            // println!("hash --> {}", key);
            (
                Regex::new(fnmatch_regex::glob_to_regex_string(&key).as_str()).unwrap(),
                value,
            )
        })
        .collect()
}

fn get_guess_paths(target_path: &Path) -> Vec<PathBuf> {
    let mut guess_paths: Vec<_> = target_path.ancestors().map(Path::to_path_buf).collect();
    if let Some(home_dir) = dirs::home_dir() {
        guess_paths.push(home_dir);
    }
    guess_paths
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
        let file_path = p.join(test_file);
        if file_path.is_file() {
            return Some(file_path);
        }
    }
    None // return None; if found nothing in paths
}

fn dedup_vec(v: &Vec<PathBuf>) -> Vec<PathBuf> {
    let mut new_vec = Vec::new();
    for i in v {
        if !new_vec.contains(i) {
            new_vec.push(i.to_path_buf());
        }
    }
    new_vec // return new_vec;
}

fn main() -> std::io::Result<()> {
    let app_options: AppOptions;
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

        app_options = AppOptions {
            enable_deletion: matches.get_flag("delete") || !matches.get_flag("no-delete"),
            enable_hash_matching: matches.get_flag("hash") || !matches.get_flag("no-hash"),
            enable_prune_empty_dir: matches.get_flag("remove-empty-dir")
                || !matches.get_flag("no-remove-empty-dir"),
            enable_renaming: matches.get_flag("rename") || !matches.get_flag("no-rename"),
            skip_parent_tmp: matches.get_flag("skip-tmp") || !matches.get_flag("no-skip-tmp"),
            prune: matches.get_flag("prune"),
            verbose: matches.get_count("verbose"),
            config_file: match matches.get_one::<PathBuf>("config") {
                None => guess_path(".cleanup-patterns.yml", get_guess_paths(&target_path)).unwrap(),
                Some(p) => p.clone(),
            },
            target_path,
        };
    }

    if app_options.is_debug_mode() {
        println!("{:#?}", app_options);
    }

    let pattern_matcher = PatternMatcher::from_config_file(&app_options.config_file);
    if app_options.is_debug_mode() {
        println!("{:#?}", pattern_matcher);
    }

    let mut operation_list: Vec<(PathBuf, String, Operation)> = vec![]; // Path, Pattern, Operation
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
        .filter_entry(|e| !app_options.skip_parent_tmp || is_not_hidden(e))
        .filter_map(|e| e.ok())
    {
        let filepath = entry.path();
        let filename = entry.file_name().to_str().unwrap();

        if app_options.enable_deletion {
            let (mut matched, mut pattern) = pattern_matcher.match_remove_pattern(filename);
            if matched {
                let p = pattern.unwrap();
                operation_list.push((filepath.to_path_buf(), p, Operation::Delete));
                continue;
            } else if app_options.enable_hash_matching {
                // test filename and hash
                (matched, pattern) = pattern_matcher.match_remove_hash(filepath.to_str().unwrap());
                if matched {
                    let p = pattern.unwrap();
                    operation_list.push((filepath.to_path_buf(), p, Operation::Delete));
                    continue;
                }
            }
        }

        if app_options.enable_renaming {
            let new_filename = pattern_matcher.clean_filename(filename);
            if new_filename != filename {
                operation_list.push((filepath.to_path_buf(), new_filename, Operation::Rename));
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
                Operation::Delete,
            ))
        }

        operation_list.push((filepath.to_path_buf(), "".to_string(), Operation::None));
    }

    if app_options.is_debug_mode() {
        println!("* operation_list: {:#?}", operation_list);
    }

    // dir tree
    if app_options.verbose >= 1 {
        print_tree(path_list_to_tree(&operation_list, &app_options.target_path));
    }

    // Remove the entries that don't require operation.
    operation_list.retain(|(_, _, op)| match op {
        Operation::None => false,
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
        for (file_path, pattern, _) in operation_list.iter().filter(|(_, _, op)| *op == Operation::Delete)
        {
            if app_options.verbose > 0 {
                println!("{} {:#?} <== {}", "[-]".red(), file_path, pattern);
            } else {
                println!("{} {:#?}", "[-]".red(), file_path);
            }

            if app_options.prune {
                if file_path.exists() {
                    remove_path(file_path.clone())?;
                }
            }
        }
    }

    if app_options.enable_renaming {
        for (file_path, new_file_name, _) in operation_list.iter().filter(|(_, _, op)| *op == Operation::Rename)
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

fn remove_path(path: PathBuf) -> std::io::Result<()> {
    match remove_file(&path) {
        Ok(()) => Ok(()),
        Err(_) => remove_dir_all(path),
    }
}

fn symbol_link_status(symbol_link_path: &Path) -> std::io::Result<(bool, PathBuf)> {
    let target = read_link(symbol_link_path)?;
    let target_path = symbol_link_path.parent().unwrap().join(&target);
    Ok((target_path.exists(), target))
}

fn path_list_to_tree(
    path_list: &Vec<(PathBuf, String, Operation)>,
    root_path: &PathBuf,
) -> Tree<String> {
    let mut tree = TreeBuilder::new()
        .with_root(format!("[root]{}", root_path.as_os_str().to_string_lossy()))
        .build();
    let mut path_node_id_map: HashMap<String, NodeId> = HashMap::new();
    let root_id = tree.root_id().unwrap();
    path_node_id_map.insert("".to_string(), root_id);

    for (path, _pattern, _op) in path_list {
        // 遍历路径的每个组件，并将每个组件添加为新的子节点
        let mut current_node_id = root_id;

        let mut parent_path = PathBuf::new();
        for p in path.strip_prefix(root_path).unwrap().components() {
            parent_path.push(p);
            let parent_path_str = parent_path.as_os_str().to_string_lossy().into_owned();
            // println!("{}", parent_path.display());
            let component_str = p.as_os_str().to_string_lossy().into_owned();

            // 检查这个组件是否已经存在
            if let Some(node_id) = path_node_id_map.get(&parent_path_str) {
                // 如果存在，则移动到下级节点
                current_node_id = *node_id;
            } else {
                // 如果不存在，则添加新的节点
                // println!("--> {:#?}", parent_path);
                let full_path = root_path.join(&parent_path);
                let (icon, name) = if full_path.is_symlink() {
                    (
                        SYMBOL_LINK,
                        match symbol_link_status(&full_path) {
                            Ok((is_valid, _target)) => {
                                format!(
                                    "{} {} {}",
                                    component_str,
                                    if is_valid {
                                        SYMBOL_LINK_ARROW.normal()
                                    } else {
                                        SYMBOL_BROKEN_ARROW.magenta()
                                    },
                                    _target.display()
                                )
                            } // express result
                            Err(_err) => "<read link ERROR>".to_string(), // express result
                        },
                    )
                } else if full_path.is_file() {
                    (SYMBOL_FILE, component_str)
                } else if full_path.is_dir() {
                    (SYMBOL_DIR, component_str + "/")
                } else {
                    ("??", component_str)
                };

                let mut parent = tree.get_mut(current_node_id).unwrap();
                let new_node = parent.append(format!("{} {}", icon, name));
                path_node_id_map.insert(parent_path_str, new_node.node_id());
                current_node_id = new_node.node_id();
            }
        }
        // println!("[DEBUG] {:#?}, {:#?}, {:#?}", parent_path, _pattern, _op);
        let _node_id = path_node_id_map
            .get(&parent_path.as_os_str().to_string_lossy().into_owned())
            .unwrap();
        let mut _node = tree.get_mut(*_node_id).unwrap();
        match _op {
            Operation::Delete => {
                let node_data = _node.data();
                *node_data = format!("{} {} <= {}", node_data, SYMBOL_DELETE.red(), _pattern);
            }
            Operation::Rename => {
                let node_data = _node.data();
                *node_data = format!("{} {} => {}", node_data, SYMBOL_RENAME.yellow(), _pattern);
            }
            _ => {}
        }
    }
    tree // return tree
}

fn print_tree(tree: Tree<String>) {
    let root_id = tree.root_id().unwrap();
    let root = tree.get(root_id).unwrap();

    // 递归地遍历树的每个节点
    fn traverse(node: &NodeRef<String>, prefix: &str) {
        let pointer = if node.parent().is_none() {
            // 根节点
            SYMBOL_ROOT
        } else if node.next_sibling().is_none() {
            // 最后一条
            GLYPH_LAST
        } else {
            GLYPH_TEE
        };
        println!("{}{}{}", prefix, pointer, node.data());

        let prefix = format!(
            "{}{}",
            prefix,
            if node.next_sibling().is_none() {
                GLYPH_TREE_SPACE
            } else {
                GLYPH_TREE_BRANCH
            }
        );
        for child in node.children() {
            traverse(&child, &prefix);
        }
    }

    traverse(&root, "");
}
