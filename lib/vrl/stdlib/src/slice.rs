use std::ops::Range;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Slice;

impl Function for Slice {
    fn identifier(&self) -> &'static str {
        "slice"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES | kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "start",
                kind: kind::INTEGER,
                required: true,
            },
            Parameter {
                keyword: "end",
                kind: kind::INTEGER,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "string start",
                source: r#"slice!("foobar", 3)"#,
                result: Ok("bar"),
            },
            Example {
                title: "string start..end",
                source: r#"slice!("foobar", 2, 4)"#,
                result: Ok("ob"),
            },
            Example {
                title: "array start",
                source: r#"slice!([0, 1, 2], 1)"#,
                result: Ok("[1, 2]"),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let start = arguments.required("start");
        let end = arguments.optional("end");

        Ok(Box::new(SliceFn { value, start, end }))
    }
}

#[derive(Debug, Clone)]
struct SliceFn {
    value: Box<dyn Expression>,
    start: Box<dyn Expression>,
    end: Option<Box<dyn Expression>>,
}

impl Expression for SliceFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let start = self.start.resolve(ctx)?.unwrap_integer();
        let end = match &self.end {
            Some(expr) => Some(expr.resolve(ctx)?.unwrap_integer()),
            None => None,
        };

        let range = |len: i64| -> Result<Range<usize>> {
            let start = match start {
                start if start < 0 => start + len,
                start => start,
            };

            let end = match end {
                Some(end) if end < 0 => end + len,
                Some(end) => end,
                None => len,
            };

            match () {
                _ if start < 0 || start > len => {
                    Err(format!(r#""start" must be between "{}" and "{}""#, -len, len).into())
                }
                _ if end < start => Err(r#""end" must be greater or equal to "start""#.into()),
                _ if end > len => Ok(start as usize..len as usize),
                _ => Ok(start as usize..end as usize),
            }
        };

        match self.value.resolve(ctx)? {
            Value::Bytes(v) => range(v.len() as i64)
                .map(|range| v.slice(range))
                .map(Value::from),
            Value::Array(mut v) => range(v.len() as i64)
                .map(|range| v.drain(range).collect::<Vec<_>>())
                .map(Value::from),
            _ => unreachable!(),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value.type_def(state).fallible()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::map;

//     vrl::test_type_def![
//         value_string {
//             expr: |_| SliceFn {
//                 value: Literal::from("foo").boxed(),
//                 start: Literal::from(0).boxed(),
//                 end: None,
//             },
//             def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
//         }

//         value_array {
//             expr: |_| SliceFn {
//                 value: array!["foo"].boxed(),
//                 start: Literal::from(0).boxed(),
//                 end: None,
//             },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Array,
//                 inner_type_def: Some(TypeDef { kind: Kind::Bytes, ..Default::default() }.boxed()),
//             },
//         }

//         value_unknown {
//             expr: |_| SliceFn {
//                 value: Variable::new("foo".to_owned(), None).boxed(),
//                 start: Literal::from(0).boxed(),
//                 end: None,
//             },
//             def: TypeDef { fallible: true, kind: Kind::Bytes | Kind::Array, ..Default::default() },
//         }
//     ];

//     #[test]
//     fn bytes() {
//         let cases = vec![
//             (
//                 map![],
//                 Ok("foo".into()),
//                 SliceFn::new(Box::new(Literal::from("foo")), 0, None),
//             ),
//             (
//                 map![],
//                 Ok("oo".into()),
//                 SliceFn::new(Box::new(Literal::from("foo")), 1, None),
//             ),
//             (
//                 map![],
//                 Ok("o".into()),
//                 SliceFn::new(Box::new(Literal::from("foo")), 2, None),
//             ),
//             (
//                 map![],
//                 Ok("oo".into()),
//                 SliceFn::new(Box::new(Literal::from("foo")), -2, None),
//             ),
//             (
//                 map![],
//                 Ok("".into()),
//                 SliceFn::new(Box::new(Literal::from("foo")), 3, None),
//             ),
//             (
//                 map![],
//                 Ok("".into()),
//                 SliceFn::new(Box::new(Literal::from("foo")), 2, Some(2)),
//             ),
//             (
//                 map![],
//                 Ok("foo".into()),
//                 SliceFn::new(Box::new(Literal::from("foo")), 0, Some(4)),
//             ),
//             (
//                 map![],
//                 Ok("oo".into()),
//                 SliceFn::new(Box::new(Literal::from("foo")), 1, Some(5)),
//             ),
//             (
//                 map![],
//                 Ok("docious".into()),
//                 SliceFn::new(
//                     Box::new(Literal::from("Supercalifragilisticexpialidocious")),
//                     -7,
//                     None,
//                 ),
//             ),
//             (
//                 map![],
//                 Ok("cali".into()),
//                 SliceFn::new(
//                     Box::new(Literal::from("Supercalifragilisticexpialidocious")),
//                     5,
//                     Some(9),
//                 ),
//             ),
//         ];

//         let mut state = state::Program::default();

//         for (object, exp, func) in cases {
//             let mut object: Value = object.into();
//             let got = func
//                 .resolve(&mut ctx)
//                 .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

//             assert_eq!(got, exp);
//         }
//     }

//     #[test]
//     fn array() {
//         let cases = vec![
//             (
//                 map![],
//                 Ok(vec![0, 1, 2].into()),
//                 SliceFn::new(Array::from(vec![0, 1, 2]).boxed(), 0, None),
//             ),
//             (
//                 map![],
//                 Ok(vec![1, 2].into()),
//                 SliceFn::new(Array::from(vec![0, 1, 2]).boxed(), 1, None),
//             ),
//             (
//                 map![],
//                 Ok(vec![1, 2].into()),
//                 SliceFn::new(Array::from(vec![0, 1, 2]).boxed(), -2, None),
//             ),
//             (
//                 map![],
//                 Ok("docious".into()),
//                 SliceFn::new(
//                     Box::new(Literal::from("Supercalifragilisticexpialidocious")),
//                     -7,
//                     None,
//                 ),
//             ),
//             (
//                 map![],
//                 Ok("cali".into()),
//                 SliceFn::new(
//                     Box::new(Literal::from("Supercalifragilisticexpialidocious")),
//                     5,
//                     Some(9),
//                 ),
//             ),
//         ];

//         let mut state = state::Program::default();

//         for (object, exp, func) in cases {
//             let mut object: Value = object.into();
//             let got = func
//                 .resolve(&mut ctx)
//                 .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

//             assert_eq!(got, exp);
//         }
//     }

//     #[test]
//     fn errors() {
//         let cases = vec![
//             (
//                 map![],
//                 Err(r#"function call error: "start" must be between "-3" and "3""#.into()),
//                 SliceFn::new(Box::new(Literal::from("foo")), 4, None),
//             ),
//             (
//                 map![],
//                 Err(r#"function call error: "start" must be between "-3" and "3""#.into()),
//                 SliceFn::new(Box::new(Literal::from("foo")), -4, None),
//             ),
//             (
//                 map![],
//                 Err(r#"function call error: "end" must be greater or equal to "start""#.into()),
//                 SliceFn::new(Box::new(Literal::from("foo")), 2, Some(1)),
//             ),
//         ];

//         let mut state = state::Program::default();

//         for (object, exp, func) in cases {
//             let mut object: Value = object.into();
//             let got = func
//                 .resolve(&mut ctx)
//                 .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

//             assert_eq!(got, exp);
//         }
//     }
// }
