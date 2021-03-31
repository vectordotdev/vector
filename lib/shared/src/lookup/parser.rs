pub use Rule as ParserRule;

#[derive(pest_derive::Parser, Default)]
#[grammar = "lookup/grammar.pest"]
pub struct Parser;
