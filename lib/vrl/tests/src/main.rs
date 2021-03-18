use vrl::{diagnostic::Formatter, state, Runtime, Value};

#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

fn main() {
    let source = ".ook = matches(s'nork')";
    let err = vrl::compile(&source, &stdlib::all()).unwrap_err();
    let formatter = Formatter::new(&source, err);

    println!("{}", formatter);
}
