use vector_common::tokenize;
use vrl::prelude::*;

fn parse_tokens(value: Value) -> Resolved {
    let string = value.try_bytes_utf8_lossy()?;
    let tokens: Value = tokenize::parse(&string)
        .into_iter()
        .map(|token| match token {
            "" | "-" => Value::Null,
            _ => token.to_owned().into(),
        })
        .collect::<Vec<_>>()
        .into();
    Ok(tokens)
}

#[derive(Clone, Copy, Debug)]
pub struct ParseTokens;

impl Function for ParseTokens {
    fn identifier(&self) -> &'static str {
        "parse_tokens"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "valid",
            source: r#"parse_tokens(s'A sentence "with \"a\" sentence inside" and [some brackets]')"#,
            result: Ok(
                r#"["A", "sentence", "with \\\"a\\\" sentence inside", "and", "some brackets"]"#,
            ),
        }]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ParseTokensFn { value }))
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        parse_tokens(value)
    }
}

#[derive(Debug, Clone)]
struct ParseTokensFn {
    value: Box<dyn Expression>,
}

impl Expression for ParseTokensFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        parse_tokens(value)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::array(Collection::from_unknown(Kind::bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        parse_tokens => ParseTokens;

        parses {
            args: func_args![value: "217.250.207.207 - - [07/Sep/2020:16:38:00 -0400] \"DELETE /deliverables/next-generation/user-centric HTTP/1.1\" 205 11881"],
            want: Ok(vec![
                            "217.250.207.207".into(),
                            Value::Null,
                            Value::Null,
                            "07/Sep/2020:16:38:00 -0400".into(),
                            "DELETE /deliverables/next-generation/user-centric HTTP/1.1".into(),
                            "205".into(),
                            "11881".into(),

                    ]),
            tdef: TypeDef::array(Collection::from_unknown(Kind::bytes())),
        }
    ];
}
