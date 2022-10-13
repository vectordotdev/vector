use std::cmp::Ordering;

use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Sort;

impl Function for Sort {
    fn identifier(&self) -> &'static str {
        "sort"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "reverse",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "sort array",
                source: r#"sort([3, 1, 2])"#,
                result: Ok("[1, 2, 3]"),
            },
            Example {
                title: "reverse sort array",
                source: r#"sort([3, 1, 2], true)"#,
                result: Ok("[3, 2, 1]"),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let reverse = arguments.optional("reverse").unwrap_or(expr!(false));

        Ok(SortFn { value, reverse }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct SortFn {
    value: Box<dyn Expression>,
    reverse: Box<dyn Expression>,
}

impl FunctionExpression for SortFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let mut value = self.value.resolve(ctx)?.try_array()?;
        let reverse = self.reverse.resolve(ctx)?.try_boolean()?;

        value.sort_by(|a, b| {
            if reverse {
                a.partial_cmp(b).unwrap_or(Ordering::Equal).reverse()
            } else {
                a.partial_cmp(b).unwrap_or(Ordering::Equal)
            }
        });

        Ok(value.into())
    }

    fn type_def(&self, state: &state::TypeState) -> TypeDef {
        let type_def = self.value.type_def(state);
        let mut collection = type_def.kind().as_array().unwrap().clone();
        let reduced_kind = collection.reduced_kind();

        for (_, kind) in collection.known_mut() {
            *kind = reduced_kind.clone();
        }

        TypeDef::array(collection).infallible()
    }
}

#[cfg(test)]
mod tests {
    use vector_common::btreemap;

    use super::*;

    test_function![
        sort => Sort;

        regular {
            args: func_args![value: value!([3, 1, 2]), reverse: value!(false)],
            want: Ok(value!([1, 2, 3])),
            tdef: TypeDef::array(btreemap! {
                Index::from(0) => Kind::integer(),
                Index::from(1) => Kind::integer(),
                Index::from(2) => Kind::integer(),
            }),
        }

        mixed {
            args: func_args![value: value!([3, "foo", true]), reverse: value!(false)],
            want: Ok(value!(["foo", 3, true])),
            tdef: TypeDef::array(btreemap! {
                Index::from(0) => Kind::integer().or_bytes().or_boolean(),
                Index::from(1) => Kind::integer().or_bytes().or_boolean(),
                Index::from(2) => Kind::integer().or_bytes().or_boolean(),
            }),
        }


        reverse {
            args: func_args![value: value!([3, 1, 2]), reverse: value!(true)],
            want: Ok(value!([3, 2, 1])),
            tdef: TypeDef::array(btreemap! {
                Index::from(0) => Kind::integer(),
                Index::from(1) => Kind::integer(),
                Index::from(2) => Kind::integer(),
            }),
        }

        strings {
            args: func_args![value: value!(["foo", "baz", "bar"]), reverse: value!(false)],
            want: Ok(value!(["bar", "baz", "foo"])),
            tdef: TypeDef::array(btreemap! {
                Index::from(0) => Kind::bytes(),
                Index::from(1) => Kind::bytes(),
                Index::from(2) => Kind::bytes(),
            }),
        }
    ];
}
