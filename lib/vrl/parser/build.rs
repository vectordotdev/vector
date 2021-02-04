extern crate lalrpop;

fn main() {
    lalrpop::Configuration::new()
        .always_use_colors()
        .process_current_dir()
        .unwrap();

    println!("cargo:rerun-if-changed=src/parser.lalrpop");
}
