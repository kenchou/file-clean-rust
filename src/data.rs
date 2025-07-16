use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub enum Operation {
    None,
    Delete,
    Rename,
    MoveToParent, // 当目录名被完全清理时，将内容移动到父目录
}

#[derive(Debug)]
pub struct AppOptions {
    pub enable_deletion: bool,
    pub enable_hash_matching: bool,
    pub enable_renaming: bool,
    pub enable_prune_empty_dir: bool,
    pub skip_parent_tmp: bool,
    pub prune: bool,
    pub verbose: u8,
    pub config_file: PathBuf,
    pub target_path: PathBuf,
}

impl AppOptions {
    pub fn is_debug_mode(&self) -> bool {
        self.verbose >= 3
    }
}
//EOP
