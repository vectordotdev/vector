use std::borrow::Cow;
use vrl::prelude::*;

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

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
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
}

#[derive(Clone, Debug)]
struct JoinFn {
    value: Box<dyn Expression>,
    separator: Option<Box<dyn Expression>>,
}

impl Expression for JoinFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let array = self.value.resolve(ctx)?.try_array()?;

        let string_vec = array
            .iter()
            .map(|s| s.try_bytes_utf8_lossy().map_err(Into::into))
            .collect::<Result<Vec<Cow<'_, str>>>>()
            .map_err(|_| "all array items must be strings")?;

        let separator: String = self
            .separator
            .as_ref()
            .map(|s| {
                s.resolve(ctx)
                    .and_then(|v| Value::try_bytes(v).map_err(Into::into))
            })
            .transpose()?
            .map(|s| String::from_utf8_lossy(&s).to_string())
            .unwrap_or_else(|| "".into());

        let joined = string_vec.join(&separator);

        Ok(Value::from(joined))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        /*let fallible = match self.value.type_def(state).kind.array() {
            Some(array) => !array.values().all(|kind| kind.is(kind::BYTES)),
            _ => true,
        };

        TypeDef::new().with_fallibility(fallible).bytes()
        */

        TypeDef::new().fallible().bytes()
    }
}

/*
#[cfg(test)]
mod test {
    use super::*;
    use value::Kind;

    test_type_def![
        value_string_array_infallible {
            expr: |_| JoinFn {
                value: array!["one", "two", "three"].boxed(),
                separator: Some(lit!(", ").boxed()),
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Bytes,
                ..Default::default()
            },
        }

        value_mixed_array_fallible {
            expr: |_| JoinFn {
                value: array!["one", 1].boxed(),
                separator: Some(lit!(", ").boxed()),
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes,
                ..Default::default()
            },
        }

        value_literal_fallible {
            expr: |_| JoinFn {
                value: lit!(427).boxed(),
                separator: None,
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes,
                ..Default::default()
            },
        }

        separator_integer_fallible {
            expr: |_| JoinFn {
                value: array!["one", "two", "three"].boxed(),
                separator: Some(lit!(427).boxed()),
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes,
                ..Default::default()
            },
        }

        both_types_wrong_fallible {
            expr: |_| JoinFn {
                value: lit!(true).boxed(),
                separator: Some(lit!(427).boxed()),
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes,
                ..Default::default()
            },
        }
    ];

    test_function![
        join => Join;

        with_comma_separator {
            args: func_args![value: array!["one", "two", "three"], separator: lit!(", ")],
            want: Ok(value!("one, two, three")),
        }

        with_space_separator {
            args: func_args![value: array!["one", "two", "three"], separator: lit!(" ")],
            want: Ok(value!("one two three")),
        }

        without_separator {
            args: func_args![value: array!["one", "two", "three"]],
            want: Ok(value!("onetwothree")),
        }

        non_string_array_item_throws_error {
            args: func_args![value: array!["one", "two", 3]],
            want: Err("function call error: all array items must be strings"),
        }
    ];
}
*/
