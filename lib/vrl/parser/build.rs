extern crate lalrpop;

fn main() {
    println!("cargo:rerun-if-changed=src/parser.lalrpop");
    lalrpop::Configuration::new()
        .always_use_colors()
        .process_current_dir()
        .unwrap();
}
