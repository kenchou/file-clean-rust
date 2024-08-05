use std::path::{Path,PathBuf};
use std::collections::HashMap;
use std::fs::read_link;

use slab_tree::{NodeId,TreeBuilder,Tree};
use colored::*;

use crate::data::Operation;

const SYMBOL_DIR: &str = "📁";
const SYMBOL_FILE: &str = "📄";
const SYMBOL_LINK: &str = "🔗";
const SYMBOL_BROKEN_ARROW: &str = "!>"; // ↛ ⥇ ⓧ ⊗ ⊘ ⤍ ⤑
const SYMBOL_LINK_ARROW: &str = "->";
const SYMBOL_DELETE: &str = "[-]"; // ␡
const SYMBOL_RENAME: &str = "[*]"; //

pub fn path_list_to_tree(
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

fn symbol_link_status(symbol_link_path: &Path) -> std::io::Result<(bool, PathBuf)> {
    let target = read_link(symbol_link_path)?;
    let target_path = symbol_link_path.parent().unwrap().join(&target);
    Ok((target_path.exists(), target))
}
//EOP
