use expression::*;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct FlatMap;

impl Function for FlatMap {
    fn identifier(&self) -> &'static str {
        "flat_map"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "field",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "flat map array",
            source: r#"flat_map([{"a": [1]}, {"a": [2]}])"#,
            result: Ok("[1, 2]"),
        }]
    }

    fn compile(&self, _state: &state::Compiler, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let field = arguments.required_literal("field")?;

        let field = if let Literal::String(b) = field {
            String::from_utf8_lossy(b.as_ref()).to_string()
        } else {
            return Err(Box::new(ExpressionError::Error {
                message: format!("Expected string literal, received {}", field),
                labels: vec![],
                notes: vec![],
            }));
        };

        Ok(Box::new(FlatMapFn { value, field }))
    }
}

#[derive(Debug, Clone)]
struct FlatMapFn {
    value: Box<dyn Expression>,
    field: String,
}

impl Expression for FlatMapFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?.try_array()?;

        let mut flattened = vec![];

        for v in value {
            if let Value::Object(mut m) = v {
                if let Some(Value::Array(arr)) = m.remove(&self.field) {
                    flattened.extend(arr);
                }
            }
        }

        Ok(flattened.into())
    }

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        flat_map => FlatMap;

        single_key {
            args: func_args![value: value!([{a: [1]}]), field: "a"],
            want: Ok(value!([1])),
            tdef: TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() }),
        }

        multiple_keys {
            args: func_args![value: value!([{a: [1]}, {a: [2]}]), field: "a"],
            want: Ok(value!([1, 2])),
            tdef: TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() }),
        }
    ];
}
