use remap::prelude::*;

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
                accepts: |v| matches!(v, Value::Array(_)),
                required: true,
            },
            Parameter {
                keyword: "separator",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let separator = arguments.optional("separator").map(Expr::boxed);

        Ok(Box::new(JoinFn { value, separator }))
    }
}

#[derive(Clone, Debug)]
struct JoinFn {
    value: Box<dyn Expression>,
    separator: Option<Box<dyn Expression>>,
}

impl Expression for JoinFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let array: Vec<Value> = self.value.execute(state, object)?
            .try_array()?;

        let string_vec: Vec<String> = self
            .value
            .execute(state, object)?
            .try_array()?
            .iter()
            .filter_map(|s| s.clone().try_bytes().ok())
            .map(|s| String::from_utf8_lossy(&s).to_string())
            .collect();

        // This length comparison is used to discern whether one or more array items was
        // discarded by the filter_map operation due to being a non-string. If so, the
        // entire operation errors; if not, string_vec is considered valid and can be
        // joined.
        if string_vec.len() < array.len() {
            Err("all array items must be strings".into())
        } else {
            let separator: String = self
                .separator
                .as_ref()
                .map(|s| {
                    s.execute(state, object)
                        .and_then(|v| Value::try_bytes(v).map_err(Into::into))
                })
                .transpose()?
                .map(|s| String::from_utf8_lossy(&s).to_string())
                .unwrap_or("".into());

            let joined = string_vec.join(&separator);

            Ok(Value::from(joined))
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        let separator_type_def = self
            .separator
            .as_ref()
            .map(|sep| sep.type_def(state).fallible_unless(Kind::Bytes));

        self
            .value
            .type_def(state)
            .merge_optional(separator_type_def)
            // Always fallible because the `value` array could contain non-strings
            .into_fallible(true)
            .with_constraint(Kind::Bytes)

    }
}

#[cfg(test)]
mod test {
    use super::*;
    use value::Kind;

    test_type_def![
        value_string_array_fallible {
            expr: |_| JoinFn {
                value: array!["one", "two", "three"].boxed(),
                separator: Some(lit!(", ").boxed()),
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes,
                ..Default::default()
            },
        }

        value_wrong_type_fallible {
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

        separator_wrong_type_fallible {
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
