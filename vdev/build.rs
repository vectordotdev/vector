use std::path::Path;

#[allow(clippy::print_stdout)]
fn main() {
    let source = Path::new("vendor/tree-sitter-vrl");
    let mut compiler = cc::Build::new();
    compiler
        .std("c11")
        .include(source)
        .file(source.join("parser.c"));
    if cfg!(target_env = "msvc") {
        compiler.flag("-utf-8");
    }
    compiler.compile("tree-sitter-vrl");
    println!("cargo:rerun-if-changed=vendor/tree-sitter-vrl/parser.c");
    println!("cargo:rerun-if-changed=vendor/tree-sitter-vrl/tree_sitter/parser.h");
}
