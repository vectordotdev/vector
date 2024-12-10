use vrl::compiler::Function;

mod parse_dnstap;

pub(crate) fn all() -> Vec<Box<dyn Function>> {
    vec![Box::new(parse_dnstap::ParseDnstap) as _]
}
