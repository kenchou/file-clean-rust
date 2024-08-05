use dirs_next as dirs;
use std::env;
use std::fs::{remove_dir_all, remove_file};
use std::path::{Path, PathBuf};
use walkdir::DirEntry;

pub fn remove_path(path: PathBuf) -> std::io::Result<()> {
    match remove_file(&path) {
        Ok(()) => Ok(()),
        Err(_) => remove_dir_all(path),
    }
}

pub fn get_guess_paths(target_path: &Path) -> Vec<PathBuf> {
    let mut guess_paths: Vec<_> = target_path.ancestors().map(Path::to_path_buf).collect();
    if let Some(home_dir) = dirs::home_dir() {
        guess_paths.push(home_dir);
    }
    guess_paths
}

pub fn is_not_hidden(entry: &DirEntry) -> bool {
    entry.file_name().to_string_lossy() != ".tmp"
        && entry.path().parent().map_or(true, |p| {
            p.file_name()
                .map_or(true, |p| p.to_string_lossy() != ".tmp")
        })
}

pub fn guess_path(test_file: &str, mut guess_paths: Vec<PathBuf>) -> Option<PathBuf> {
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

pub fn dedup_vec(v: &Vec<PathBuf>) -> Vec<PathBuf> {
    let mut new_vec = Vec::new();
    for i in v {
        if !new_vec.contains(i) {
            new_vec.push(i.to_path_buf());
        }
    }
    new_vec // return new_vec;
}
//EOP
