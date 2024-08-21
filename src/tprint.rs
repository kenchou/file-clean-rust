use nary_tree::{NodeRef, Tree};

const GLYPH_TREE_SPACE: &str = "    ";
const GLYPH_TREE_BRANCH: &str = "│   ";
const GLYPH_TEE: &str = "├── ";
const GLYPH_LAST: &str = "└── ";

const SYMBOL_ROOT: &str = "📂";

pub fn print_tree(tree: Tree<String>) {
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
//EOP
