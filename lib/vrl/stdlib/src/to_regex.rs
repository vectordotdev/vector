use regex::Regex;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToRegex;

impl Function for ToRegex {
    fn identifier(&self) -> &'static str {
        "to_regex"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::bytes,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "regex",
                source: "to_regex(s'foo')",
                result: Ok(r"foo"),
            }
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        Ok(Box::new(ToRegexFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ToRegexFn {
    value: Box<dyn Expression>,
}

impl Expression for ToRegexFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Value::*;

        let value = match self.value.resolve(ctx)? {
            v @ Bytes(_) => Value::Regex::new(v).unwrap(),
            v => return Err(format!(r#"unable to coerce {} into "regex""#, v.kind()).into()),
        };

        Ok(value)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(
                Kind::Bytes
            )
            .bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        to_regex => ToRegex;

        plaintext {
            args: func_args![value: "foo"],
            want: Ok(Regex::new("foo").unwrap()),
            tdef: Regex::new("foo").unwrap(),
        }
    ];
}
