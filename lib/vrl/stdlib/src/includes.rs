use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Includes;

impl Function for Includes {
    fn identifier(&self) -> &'static str {
        "includes"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "item",
                kind: kind::ANY,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "includes",
                source: r#"includes([1, true], true)"#,
                result: Ok("true"),
            },
            Example {
                title: "no includes",
                source: r#"includes(["foo", "bar"], "baz")"#,
                result: Ok("false"),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let item = arguments.required("item");

        Ok(Box::new(IncludesFn { value, item }))
    }
}

#[derive(Debug, Clone)]
struct IncludesFn {
    value: Box<dyn Expression>,
    item: Box<dyn Expression>,
}

impl Expression for IncludesFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let list = self.value.resolve(ctx)?.unwrap_array();
        let item = self.item.resolve(ctx)?;

        let included = list.iter().any(|i| i == &item);

        Ok(included.into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     test_type_def![
//         value_non_empty_array_infallible {
//             expr: |_| IncludesFn {
//                 value: Array::from(vec!["foo", "bar", "baz"]).boxed(),
//                 item: Literal::from("foo").boxed(),
//             },
//             def: TypeDef { kind: Kind::Boolean, ..Default::default() },
//         }

//         value_not_an_array_fallible {
//             expr: |_| IncludesFn {
//                 value: Literal::from("foo").boxed(), // Must be an array, hence fallible
//                 item: Literal::from("foo").boxed(),
//             },
//             def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
//         }
//     ];

//     test_function![
//         includes => Includes;

//         empty_not_included {
//             args: func_args![value: value!([]), item: value!("foo")],
//             want: Ok(value!(false)),
//         }

//         string_included {
//             args: func_args![value: value!(["foo", "bar"]), item: value!("foo")],
//             want: Ok(value!(true)),
//         }

//         string_not_included {
//             args: func_args![value: value!(["foo", "bar"]), item: value!("baz")],
//             want: Ok(value!(false)),
//         }

//         bool_included {
//             args: func_args![value: value!([true, false]), item: value!(true)],
//             want: Ok(value!(true)),
//         }

//         bool_not_included {
//             args: func_args![value: value!([true, true]), item: value!(false)],
//             want: Ok(value!(false)),
//         }

//         integer_included {
//             args: func_args![value: value!([1, 2, 3, 4, 5]), item: value!(5)],
//             want: Ok(value!(true)),
//         }

//         integer_not_included {
//             args: func_args![value: value!([1, 2, 3, 4, 6]), item: value!(5)],
//             want: Ok(value!(false)),
//         }

//         float_included {
//             args: func_args![value: value!([0.5, 12.1, 13.075]), item: value!(13.075)],
//             want: Ok(value!(true)),
//         }

//         float_not_included {
//             args: func_args![value: value!([0.5, 12.1, 13.075]), item: value!(471.0)],
//             want: Ok(value!(false)),
//         }

//         array_included {
//             args: func_args![value: value!([[1,2,3], [4,5,6]]), item: value!([1,2,3])],
//             want: Ok(value!(true)),
//         }

//         array_not_included {
//             args: func_args![value: value!([[1,2,3], [4,5,6]]), item: value!([1,2,4])],
//             want: Ok(value!(false)),
//         }

//         mixed_included_string {
//             args: func_args![value: value!(["foo", 1, true, [1,2,3]]), item: value!("foo")],
//             want: Ok(value!(true)),
//         }

//         mixed_not_included_string {
//             args: func_args![value: value!(["bar", 1, true, [1,2,3]]), item: value!("foo")],
//             want: Ok(value!(false)),
//         }

//         mixed_included_bool {
//             args: func_args![value: value!(["foo", 1, true, [1,2,3]]), item: value!(true)],
//             want: Ok(value!(true)),
//         }

//         mixed_not_included_bool {
//             args: func_args![value: value!(["foo", 1, true, [1,2,3]]), item: value!(false)],
//             want: Ok(value!(false)),
//         }
//     ];
// }
