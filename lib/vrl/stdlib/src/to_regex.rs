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
        let value = self.value.resolve(ctx)?;
        let string = value.try_bytes_utf8_lossy()?;
        let regex = regex::Regex::new(string.as_ref())
            .map_err(|err| format!("could not create regex: {}", err))
            .map(Into::into)?;
        Ok(regex)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value.type_def(state).fallible().regex()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        to_regex => ToRegex;

        plaintext {
            args: func_args![value: "^foobar$"],
            want: Ok(regex::Regex::new("^foobar$")),
            tdef: TypeDef::new().regex(),
        }
    ];
}
