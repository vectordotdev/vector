use std::convert::AsRef;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Operator {
    Multiply,
    Divide,
    IntegerDivide,
    Remainder,
    Add,
    Subtract,
    Equal,
    NotEqual,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
    And,
    Or,
}

impl FromStr for Operator {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        use Operator::*;

        Ok(match s {
            "*" => Multiply,
            "/" => Divide,
            "//" => IntegerDivide,
            "%" => Remainder,
            "+" => Add,
            "-" => Subtract,
            "==" => Equal,
            "!=" => NotEqual,
            ">" => Greater,
            ">=" => GreaterOrEqual,
            "<" => Less,
            "<=" => LessOrEqual,
            "&&" => And,
            "||" => Or,
            _ => return Err("unknown operator"),
        })
    }
}

impl AsRef<str> for Operator {
    fn as_ref(&self) -> &'static str {
        use Operator::*;

        match self {
            Multiply => "*",
            Divide => "/",
            IntegerDivide => "//",
            Remainder => "%",
            Add => "+",
            Subtract => "-",
            Equal => "==",
            NotEqual => "!=",
            Greater => ">",
            GreaterOrEqual => ">=",
            Less => "<",
            LessOrEqual => "<=",
            And => "&&",
            Or => "||",
        }
    }
}
