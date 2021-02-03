use remap::prelude::*;

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
                accepts: |v| matches!(v, Value::Bytes(_) | Value::Array(_)),
                required: true,
            },
            Parameter {
                keyword: "start",
                accepts: |v| matches!(v, Value::Integer(_)),
                required: true,
            },
            Parameter {
                keyword: "end",
                accepts: |v| matches!(v, Value::Integer(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let start = arguments.required("start")?.boxed();
        let end = arguments.optional("end").map(Expr::boxed);

        Ok(Box::new(SliceFn { value, start, end }))
    }
}

#[derive(Debug, Clone)]
struct SliceFn {
    value: Box<dyn Expression>,
    start: Box<dyn Expression>,
    end: Option<Box<dyn Expression>>,
}

impl SliceFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, start: isize, end: Option<isize>) -> Self {
        let start = Box::new(Literal::from(start as i64));
        let end = end.map(|i| Box::new(Literal::from(i as i64)) as _);

        Self { value, start, end }
    }
}

impl Expression for SliceFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let start = self.start.execute(state, object)?.try_integer()?;
        let end = match &self.end {
            Some(expr) => Some(expr.execute(state, object)?.try_integer()?),
            None => None,
        };

        let range = |len: i64| -> Result<std::ops::Range<usize>> {
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

        match self.value.execute(state, object)? {
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
        use value::Kind;

        let value_def = self
            .value
            .type_def(state)
            .fallible_unless(Kind::Bytes | Kind::Array);
        let end_def = self
            .end
            .as_ref()
            .map(|end| end.type_def(state).fallible_unless(Kind::Integer));

        value_def
            .clone()
            .merge(self.start.type_def(state).fallible_unless(Kind::Integer))
            .merge_optional(end_def)
            .with_constraint(match value_def.kind {
                v if v.is_bytes() || v.is_array() => v,
                _ => Kind::Bytes | Kind::Array,
            })
            .into_fallible(true) // can fail for invalid start..end ranges
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;
    use value::Kind;

    remap::test_type_def![
        value_string {
            expr: |_| SliceFn {
                value: Literal::from("foo").boxed(),
                start: Literal::from(0).boxed(),
                end: None,
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        value_array {
            expr: |_| SliceFn {
                value: array!["foo"].boxed(),
                start: Literal::from(0).boxed(),
                end: None,
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Array,
                inner_type_def: Some(inner_type_def!([ Kind::Bytes ])),
            },
        }

        value_unknown {
            expr: |_| SliceFn {
                value: Variable::new("foo".to_owned(), None).boxed(),
                start: Literal::from(0).boxed(),
                end: None,
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes | Kind::Array, ..Default::default() },
        }
    ];

    #[test]
    fn bytes() {
        let cases = vec![
            (
                btreemap! {},
                Ok("foo".into()),
                SliceFn::new(Box::new(Literal::from("foo")), 0, None),
            ),
            (
                btreemap! {},
                Ok("oo".into()),
                SliceFn::new(Box::new(Literal::from("foo")), 1, None),
            ),
            (
                btreemap! {},
                Ok("o".into()),
                SliceFn::new(Box::new(Literal::from("foo")), 2, None),
            ),
            (
                btreemap! {},
                Ok("oo".into()),
                SliceFn::new(Box::new(Literal::from("foo")), -2, None),
            ),
            (
                btreemap! {},
                Ok("".into()),
                SliceFn::new(Box::new(Literal::from("foo")), 3, None),
            ),
            (
                btreemap! {},
                Ok("".into()),
                SliceFn::new(Box::new(Literal::from("foo")), 2, Some(2)),
            ),
            (
                btreemap! {},
                Ok("foo".into()),
                SliceFn::new(Box::new(Literal::from("foo")), 0, Some(4)),
            ),
            (
                btreemap! {},
                Ok("oo".into()),
                SliceFn::new(Box::new(Literal::from("foo")), 1, Some(5)),
            ),
            (
                btreemap! {},
                Ok("docious".into()),
                SliceFn::new(
                    Box::new(Literal::from("Supercalifragilisticexpialidocious")),
                    -7,
                    None,
                ),
            ),
            (
                btreemap! {},
                Ok("cali".into()),
                SliceFn::new(
                    Box::new(Literal::from("Supercalifragilisticexpialidocious")),
                    5,
                    Some(9),
                ),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }

    #[test]
    fn array() {
        let cases = vec![
            (
                btreemap! {},
                Ok(vec![0, 1, 2].into()),
                SliceFn::new(Array::from(vec![0, 1, 2]).boxed(), 0, None),
            ),
            (
                btreemap! {},
                Ok(vec![1, 2].into()),
                SliceFn::new(Array::from(vec![0, 1, 2]).boxed(), 1, None),
            ),
            (
                btreemap! {},
                Ok(vec![1, 2].into()),
                SliceFn::new(Array::from(vec![0, 1, 2]).boxed(), -2, None),
            ),
            (
                btreemap! {},
                Ok("docious".into()),
                SliceFn::new(
                    Box::new(Literal::from("Supercalifragilisticexpialidocious")),
                    -7,
                    None,
                ),
            ),
            (
                btreemap! {},
                Ok("cali".into()),
                SliceFn::new(
                    Box::new(Literal::from("Supercalifragilisticexpialidocious")),
                    5,
                    Some(9),
                ),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }

    #[test]
    fn errors() {
        let cases = vec![
            (
                btreemap! {},
                Err(r#"function call error: "start" must be between "-3" and "3""#.into()),
                SliceFn::new(Box::new(Literal::from("foo")), 4, None),
            ),
            (
                btreemap! {},
                Err(r#"function call error: "start" must be between "-3" and "3""#.into()),
                SliceFn::new(Box::new(Literal::from("foo")), -4, None),
            ),
            (
                btreemap! {},
                Err(r#"function call error: "end" must be greater or equal to "start""#.into()),
                SliceFn::new(Box::new(Literal::from("foo")), 2, Some(1)),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
