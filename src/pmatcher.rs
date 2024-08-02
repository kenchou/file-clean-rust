use std::path::{Path,PathBuf};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

use fancy_regex::Regex;
use md5::{Digest, Md5};

use crate::pconfig;
use crate::fnmatch_regex;

#[derive(Debug)]
pub struct PatternMatcher {
    pub patterns_to_remove: Vec<Regex>,
    pub patterns_to_remove_with_hash: Vec<(Regex, Vec<String>)>,
    pub patterns_to_rename: Vec<Regex>,
}

impl PatternMatcher {
    pub fn from_config_file(config_file: &Path) -> PatternMatcher {
        let config = pconfig::PatternsConfig::from_config_file(config_file);
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

    pub fn match_remove_pattern(&self, test_file: &str) -> (bool, Option<String>) {
        for re in &self.patterns_to_remove {
            if re.is_match(test_file).unwrap() {
                return (true, Some(re.to_string()));
            }
        }
        (false, None) // return
    }

    pub fn match_remove_hash(&self, test_file: &str) -> (bool, Option<String>) {
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

    pub fn clean_filename(&self, filename: &str) -> String {
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
//EOP
