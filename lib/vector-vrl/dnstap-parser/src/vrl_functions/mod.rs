use vrl::compiler::Function;

pub mod parse_dnstap;

pub fn all() -> Vec<Box<dyn Function>> {
    vec![Box::new(parse_dnstap::ParseDnstap) as _]
}
