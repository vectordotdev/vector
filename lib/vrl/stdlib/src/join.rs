use std::borrow::Cow;

use ::value::Value;
use vrl::prelude::*;

fn join(array: Value, separator: Option<Value>) -> Resolved {
    let array = array.try_array()?;
    let string_vec = array
        .iter()
        .map(|s| s.try_bytes_utf8_lossy().map_err(Into::into))
        .collect::<Result<Vec<Cow<'_, str>>>>()
        .map_err(|_| "all array items must be strings")?;
    let separator: String = separator
        .map(Value::try_bytes)
        .transpose()?
        .map(|s| String::from_utf8_lossy(&s).to_string())
        .unwrap_or_else(|| "".into());
    let joined = string_vec.join(&separator);
    Ok(Value::from(joined))
}

#[derive(Clone, Copy, Debug)]
pub struct Join;

impl Function for Join {
    fn identifier(&self) -> &'static str {
        "join"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "separator",
                kind: kind::BYTES,
                required: false,
            },
        ]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let separator = arguments.optional("separator");

        Ok(Box::new(JoinFn { value, separator }))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "join",
            source: r#"join!(["a","b","c"], ",")"#,
            result: Ok(r#"a,b,c"#),
        }]
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        let separator = args.optional("separator");

        join(value, separator)
    }
}

#[derive(Clone, Debug)]
struct JoinFn {
    value: Box<dyn Expression>,
    separator: Option<Box<dyn Expression>>,
}

impl Expression for JoinFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let array = self.value.resolve(ctx)?;
        let separator = self
            .separator
            .as_ref()
            .map(|s| s.resolve(ctx))
            .transpose()?;

        join(array, separator)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::bytes().fallible()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    test_function![
        join => Join;

        with_comma_separator {
            args: func_args![value: value!(["one", "two", "three"]), separator: ", "],
            want: Ok(value!("one, two, three")),
            tdef: TypeDef::bytes().fallible(),
        }

        with_space_separator {
            args: func_args![value: value!(["one", "two", "three"]), separator: " "],
            want: Ok(value!("one two three")),
            tdef: TypeDef::bytes().fallible(),
        }

        without_separator {
            args: func_args![value: value!(["one", "two", "three"])],
            want: Ok(value!("onetwothree")),
            tdef: TypeDef::bytes().fallible(),
        }

        non_string_array_item_throws_error {
            args: func_args![value: value!(["one", "two", 3])],
            want: Err("all array items must be strings"),
            tdef: TypeDef::bytes().fallible(),
        }
    ];
}
