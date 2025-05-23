use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};

use fancy_regex::Regex;
use indicatif::ProgressBar;
use md5::{Digest, Md5};

use crate::fnmatch_regex;
use crate::pconfig;

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

    #[allow(dead_code)]
    pub fn match_remove_hash(&self, test_file: &str) -> (bool, Option<String>) {
        let filepath = Path::new(test_file);
        let filename = match filepath.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return (false, None), // 避免无效文件名
        };
        for (re, hash_list) in &self.patterns_to_remove_with_hash {
            if re.is_match(filename).unwrap() {
                // 跳过大文件检查
                if let Ok(metadata) = std::fs::metadata(filepath) {
                    if metadata.len() > 100 * 1024 * 1024 {
                        return (false, None);
                    }
                }
                // 处理 Result 类型
                if let Ok(hash) = calculate_md5(test_file) {
                    if hash_list.contains(&hash) {
                        return (true, Some(format!("{}:{}", re, hash)));
                    }
                }
            }
        }
        (false, None)
    }

    #[allow(dead_code)]
    pub fn match_remove_hash_with_progress(
        &self,
        test_file: &str,
        progress: Option<&ProgressBar>,
    ) -> (bool, Option<String>) {
        let filepath = Path::new(test_file);
        let filename = match filepath.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return (false, None), // 避免无效文件名
        };

        // 避免频繁更新和过长消息
        let mut last_update = std::time::Instant::now();

        for (re, hash_list) in &self.patterns_to_remove_with_hash {
            if re.is_match(filename).unwrap() {
                // 跳过大文件检查
                if let Ok(metadata) = std::fs::metadata(filepath) {
                    if metadata.len() > 100 * 1024 * 1024 {
                        return (false, None);
                    }
                }

                // 限制频率更新消息，避免栈溢出
                if let Some(pb) = progress {
                    let now = std::time::Instant::now();
                    if now.duration_since(last_update).as_millis() > 100 {
                        // 截断文件名以避免过长
                        let short_name = if filename.len() > 50 {
                            format!("{}...", &filename[0..47])
                        } else {
                            filename.to_string()
                        };
                        pb.set_message(format!("计算MD5: {}", short_name));
                        last_update = now;
                    }
                }

                if let Ok(hash) = calculate_md5(test_file) {
                    if hash_list.contains(&hash) {
                        return (true, Some(format!("{}:{}", re, hash)));
                    }
                }
            }
        }
        (false, None)
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

fn calculate_md5(filepath: &str) -> io::Result<String> {
    let file = File::open(filepath)?;
    let mut reader = BufReader::with_capacity(8 * 1024 * 1024, file);

    // 使用堆分配的 Vec 代替栈上的大数组
    let mut buffer = vec![0; 64 * 1024]; // 64KB 缓冲区，在堆上分配
    let mut hasher = Md5::new();

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn create_patterns_with_hash(patterns: HashMap<String, Vec<String>>) -> Vec<(Regex, Vec<String>)> {
    patterns
        .into_iter()
        .map(|(key, value)| (parse_mixed_regex(&key), value))
        .collect()
}

fn parse_mixed_regex(pattern: &str) -> Regex {
    let pattern = pattern.trim();
    // println!(">>> {:#?}", pattern);
    if let Some(stripped) = pattern.strip_prefix('/') {
        Regex::new(stripped).unwrap()
    } else {
        Regex::new(fnmatch_regex::glob_to_regex_string(pattern).as_str()).unwrap()
    }
}

/**
 * 创建正则表达式列表，通配符形式转为正则表达式
 */
fn create_mixed_regex_list(patterns: Vec<&str>) -> Vec<Regex> {
    patterns
        .iter()
        .map(|pattern| parse_mixed_regex(pattern))
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
