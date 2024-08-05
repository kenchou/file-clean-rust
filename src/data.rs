use std::path::PathBuf;

#[derive(Debug, PartialEq)]
pub enum Operation {
    None,
    Delete,
    Rename,
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
