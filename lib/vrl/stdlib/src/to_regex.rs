use ::value::Value;
use tracing::warn;
use vrl::prelude::*;

fn to_regex(value: Value) -> Resolved {
    let string = value.try_bytes_utf8_lossy()?;
    let regex = regex::Regex::new(string.as_ref())
        .map_err(|err| format!("could not create regex: {err}"))
        .map(Into::into)?;
    Ok(regex)
}

#[derive(Clone, Copy, Debug)]
pub struct ToRegex;

impl Function for ToRegex {
    fn identifier(&self) -> &'static str {
        "to_regex"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "regex",
            source: "to_regex(s'^foobar$') ?? r''",
            result: Ok("r'^foobar$'"),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        warn!("`to_regex` is an expensive function that could impact throughput.");
        let value = arguments.required("value");
        Ok(ToRegexFn { value }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct ToRegexFn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for ToRegexFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        to_regex(value)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::regex().fallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        to_regex => ToRegex;

        regex {
            args: func_args![value: "^test[A-Za-z_]+$"],
            want: Ok(regex::Regex::new("^test[A-Za-z_]+$").expect("regex is valid")),
            tdef: TypeDef::regex().fallible(),
        }

        invalid_regex {
            args: func_args![value: "(+)"],
            want: Err("could not create regex: regex parse error:\n    (+)\n     ^\nerror: repetition operator missing expression"),
            tdef: TypeDef::regex().fallible(),
        }
    ];
}
