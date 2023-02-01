extern crate lalrpop;

fn main() {
    lalrpop::Configuration::new()
        .always_use_colors()
        .emit_rerun_directives(true)
        .emit_whitespace(false)
        .process_current_dir()
        .unwrap();
}
