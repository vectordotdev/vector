extern crate lalrpop;

fn main() {
    lalrpop::process_root().expect("couldn't process grammar");
}
