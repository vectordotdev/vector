extern crate lalrpop;

fn main() {
    println!("cargo:rerun-if-changed=src/path.lalrpop");

    lalrpop::Configuration::new()
        .always_use_colors()
        .process_current_dir()
        .unwrap();
}
