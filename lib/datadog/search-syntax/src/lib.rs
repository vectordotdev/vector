#[macro_use]
extern crate pest_derive;

mod grammar;
mod node;
mod parser;
mod vrl;

pub use node::QueryNode;
pub use parser::parse;

#[cfg(test)]
mod tests {
    use super::parse;
    use vrl_parser::ast;

    #[test]
    fn to_vrl() {
        let query_string = "@a:*tes*t";

        match parse(query_string) {
            Err(e) => println!("Unable to parse query: {}", e),
            Ok(node) => {
                // Do a bunch of generic output so we can see what we did here
                println!("Original: [[{}]]", query_string);
                println!("Parsed: {:#?}", node);
                println!("Lucene: '{}'", node.to_lucene());
                println!("VRL: '{}'", Into::<ast::Expr>::into(node));
            }
        }
    }
}
