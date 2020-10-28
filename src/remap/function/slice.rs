use remap::prelude::*;

#[derive(Debug)]
pub struct Slice;

impl Function for Slice {
    fn identifier(&self) -> &'static str {
        "slice"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::String(_) | Value::Array(_)),
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
        let value = arguments.required_expr("value")?;
        let start = arguments.required_expr("start")?;
        let end = arguments.optional_expr("end")?;

        Ok(Box::new(SliceFn { value, start, end }))
    }
}

#[derive(Debug)]
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
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let start = required!(state, object, self.start, Value::Integer(v) => v);
        let end = optional!(state, object, self.end, Value::Integer(v) => v);

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

        required! {
            state, object, self.value,
            Value::String(v) => range(v.len() as i64)
                .map(|range| v.slice(range))
                .map(Value::from)
                .map(Some),
            Value::Array(mut v) => range(v.len() as i64)
                .map(|range| v.drain(range).collect::<Vec<_>>())
                .map(Value::from)
                .map(Some),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn bytes() {
        let cases = vec![
            (
                map![],
                Ok(Some("foo".into())),
                SliceFn::new(Box::new(Literal::from("foo")), 0, None),
            ),
            (
                map![],
                Ok(Some("oo".into())),
                SliceFn::new(Box::new(Literal::from("foo")), 1, None),
            ),
            (
                map![],
                Ok(Some("o".into())),
                SliceFn::new(Box::new(Literal::from("foo")), 2, None),
            ),
            (
                map![],
                Ok(Some("oo".into())),
                SliceFn::new(Box::new(Literal::from("foo")), -2, None),
            ),
            (
                map![],
                Ok(Some("".into())),
                SliceFn::new(Box::new(Literal::from("foo")), 3, None),
            ),
            (
                map![],
                Ok(Some("".into())),
                SliceFn::new(Box::new(Literal::from("foo")), 2, Some(2)),
            ),
            (
                map![],
                Ok(Some("foo".into())),
                SliceFn::new(Box::new(Literal::from("foo")), 0, Some(4)),
            ),
            (
                map![],
                Ok(Some("oo".into())),
                SliceFn::new(Box::new(Literal::from("foo")), 1, Some(5)),
            ),
            (
                map![],
                Ok(Some("docious".into())),
                SliceFn::new(
                    Box::new(Literal::from("Supercalifragilisticexpialidocious")),
                    -7,
                    None,
                ),
            ),
            (
                map![],
                Ok(Some("cali".into())),
                SliceFn::new(
                    Box::new(Literal::from("Supercalifragilisticexpialidocious")),
                    5,
                    Some(9),
                ),
            ),
        ];

        let mut state = remap::State::default();

        for (mut object, exp, func) in cases {
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
                map![],
                Ok(Some(vec![0, 1, 2].into())),
                SliceFn::new(Box::new(Literal::from(vec![0, 1, 2])), 0, None),
            ),
            (
                map![],
                Ok(Some(vec![1, 2].into())),
                SliceFn::new(Box::new(Literal::from(vec![0, 1, 2])), 1, None),
            ),
            (
                map![],
                Ok(Some(vec![1, 2].into())),
                SliceFn::new(Box::new(Literal::from(vec![0, 1, 2])), -2, None),
            ),
            (
                map![],
                Ok(Some("docious".into())),
                SliceFn::new(
                    Box::new(Literal::from("Supercalifragilisticexpialidocious")),
                    -7,
                    None,
                ),
            ),
            (
                map![],
                Ok(Some("cali".into())),
                SliceFn::new(
                    Box::new(Literal::from("Supercalifragilisticexpialidocious")),
                    5,
                    Some(9),
                ),
            ),
        ];

        let mut state = remap::State::default();

        for (mut object, exp, func) in cases {
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
                map![],
                Err("path error: missing path: foo".into()),
                SliceFn::new(Box::new(Path::from("foo")), 0, None),
            ),
            (
                map![],
                Err(r#"function call error: "start" must be between "-3" and "3""#.into()),
                SliceFn::new(Box::new(Literal::from("foo")), 4, None),
            ),
            (
                map![],
                Err(r#"function call error: "start" must be between "-3" and "3""#.into()),
                SliceFn::new(Box::new(Literal::from("foo")), -4, None),
            ),
            (
                map![],
                Err(r#"function call error: "end" must be greater or equal to "start""#.into()),
                SliceFn::new(Box::new(Literal::from("foo")), 2, Some(1)),
            ),
        ];

        let mut state = remap::State::default();

        for (mut object, exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
