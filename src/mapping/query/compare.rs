use super::Function;
use crate::{
    event::{util::log::get_value, Event, PathIter, Value},
    mapping::Result,
};
use string_cache::DefaultAtom as Atom;

pub enum CompareOperator {
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
    Equal,
    NotEqual,
}

#[derive(Debug)]
pub struct Compare {
    left: Box<dyn Function>,
    right: Box<dyn Function>,
    op: CompareOperator,
}

impl Compare {
    fn new(left: Box<dyn Function>, right: Box<dyn Function>, op: CompareOperator) -> Self {
        Self { left, right, op }
    }
}

impl Function for Path {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        Err("not implemented".into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    #[test]
    fn check_compare_query() {
        let cases = vec![
            (
                Event::from(""),
                Err("not implemented".to_string()),
                Compare::new(Path::from(vec![vec!["foo"]]), Path::from(vec![vec!["bar"]]), CompareOperator::Equal),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
