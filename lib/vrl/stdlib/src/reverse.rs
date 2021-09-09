use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Reverse;

impl Function for Reverse {
    fn identifier(&self) -> &'static str {
        "reverse"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ARRAY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "reverse array",
            source: r#"reverse([0, 1])"#,
            result: Ok("[1, 0]"),
        }]
    }

    fn compile(&self, _state: &state::Compiler, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ReverseFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ReverseFn {
    value: Box<dyn Expression>,
}

impl Expression for ReverseFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let mut value = self.value.resolve(ctx)?.try_array()?;

        value.reverse();

        Ok(value.into())
    }

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        reverse => Reverse;

        both_arrays_empty {
            args: func_args![value: value!([]), items: value!([])],
            want: Ok(value!([])),
            tdef: TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() }),
        }

        one_array_empty {
            args: func_args![value: value!([1, 2, 3])],
            want: Ok(value!([3, 2, 1])),
            tdef: TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() }),
        }

        mixed_array_types {
            args: func_args![value: value!([1, 2, 3, true, 5.0, "bar"])],
            want: Ok(value!(["bar", 5.0, true, 3, 2, 1])),
            tdef: TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() }),
        }
    ];
}
