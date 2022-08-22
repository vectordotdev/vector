extern crate lalrpop;

fn main() {
    build_larlpop();
    // build_tree_sitter();
}

fn build_larlpop() {
    lalrpop::Configuration::new()
        .always_use_colors()
        .emit_rerun_directives(true)
        .emit_whitespace(false)
        .process_current_dir()
        .unwrap();
}

fn build_tree_sitter() {
    use std::path::PathBuf;
    let dir: PathBuf = ["tree-sitter-vrl", "src"].iter().collect();

    cc::Build::new()
        .include(&dir)
        .file(dir.join("parser.c"))
        .compile("tree-sitter-vrl");
}
