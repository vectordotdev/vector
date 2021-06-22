use lookup::LookupBuf;
use vrl::Value;

#[derive(Clone, Debug)]
pub struct GrokPattern {
    pub match_fn: Function,
    pub destination: Option<Destination>,
}

#[derive(Clone, Debug)]
pub struct Destination {
    pub path: LookupBuf,
    pub filter_fn: Option<Function>,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub name: String,
    pub args: Option<Vec<FunctionArgument>>,
}

impl Function {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            args: None,
        }
    }

    pub fn new_with_args(name: &str, args: Option<Vec<FunctionArgument>>) -> Self {
        Self {
            name: name.to_string(),
            args,
        }
    }
}

#[derive(Clone, Debug)]
pub enum FunctionArgument {
    FUNCTION(Function),
    ARG(Value),
}
