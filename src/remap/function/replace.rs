use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Replace;

impl Function for Replace {
    fn identifier(&self) -> &'static str {
        "replace"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
            Parameter {
                keyword: "pattern",
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
            Parameter {
                keyword: "with",
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
            Parameter {
                keyword: "count",
                accepts: |v| matches!(v, Value::Integer(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let pattern = arguments.required("pattern")?;
        let with = arguments.required_expr("with")?;
        let count = arguments.optional_expr("count")?;

        Ok(Box::new(ReplaceFn {
            value,
            pattern,
            with,
            count,
        }))
    }
}

#[derive(Debug, Clone)]
struct ReplaceFn {
    value: Box<dyn Expression>,
    pattern: Argument,
    with: Box<dyn Expression>,
    count: Option<Box<dyn Expression>>,
}

impl ReplaceFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, pattern: Argument, with: &str, count: Option<i32>) -> Self {
        let with = Box::new(Literal::from(Value::from(with)));
        let count = count.map(Literal::from).map(|v| Box::new(v) as _);

        ReplaceFn {
            value,
            pattern,
            with,
            count,
        }
    }
}

impl Expression for ReplaceFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let value = required!(state, object, self.value, Value::String(b) => String::from_utf8_lossy(&b).into_owned());
        let with = required!(state, object, self.with, Value::String(b) => String::from_utf8_lossy(&b).into_owned());
        let count = optional!(state, object, self.count, Value::Integer(v) => v).unwrap_or(-1);

        match &self.pattern {
            Argument::Expression(expr) => {
                let pattern = required!(state, object, expr, Value::String(b) => String::from_utf8_lossy(&b).into_owned());
                let replaced = match count {
                    i if i > 0 => value.replacen(&pattern, &with, i as usize),
                    i if i < 0 => value.replace(&pattern, &with),
                    _ => value,
                };

                Ok(Some(replaced.into()))
            }
            Argument::Regex(regex) => {
                let replaced = match count {
                    i if i > 0 => regex
                        .replacen(&value, i as usize, with.as_str())
                        .as_bytes()
                        .into(),
                    i if i < 0 => regex.replace_all(&value, with.as_str()).as_bytes().into(),
                    _ => value.into(),
                };

                Ok(Some(replaced))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::map;

    #[test]
    fn check_replace_string() {
        let cases = vec![
            (
                map![],
                Ok(Some("I like opples ond bononos".into())),
                ReplaceFn::new(
                    Box::new(Literal::from("I like apples and bananas")),
                    Argument::Expression(Box::new(Literal::from("a"))),
                    "o",
                    None,
                ),
            ),
            (
                map![],
                Ok(Some("I like opples ond bononos".into())),
                ReplaceFn::new(
                    Box::new(Literal::from("I like apples and bananas")),
                    Argument::Expression(Box::new(Literal::from("a"))),
                    "o",
                    Some(-1),
                ),
            ),
            (
                map![],
                Ok(Some("I like apples and bananas".into())),
                ReplaceFn::new(
                    Box::new(Literal::from("I like apples and bananas")),
                    Argument::Expression(Box::new(Literal::from("a"))),
                    "o",
                    Some(0),
                ),
            ),
            (
                map![],
                Ok(Some("I like opples and bananas".into())),
                ReplaceFn::new(
                    Box::new(Literal::from("I like apples and bananas")),
                    Argument::Expression(Box::new(Literal::from("a"))),
                    "o",
                    Some(1),
                ),
            ),
            (
                map![],
                Ok(Some("I like opples ond bananas".into())),
                ReplaceFn::new(
                    Box::new(Literal::from("I like apples and bananas")),
                    Argument::Expression(Box::new(Literal::from("a"))),
                    "o",
                    Some(2),
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
    fn check_replace_regex() {
        let cases = vec![
            (
                map![],
                Ok(Some("I like opples ond bononos".into())),
                ReplaceFn::new(
                    Box::new(Literal::from("I like apples and bananas")),
                    Argument::Regex(regex::Regex::new("a").unwrap()),
                    "o",
                    None,
                ),
            ),
            (
                map![],
                Ok(Some("I like opples ond bononos".into())),
                ReplaceFn::new(
                    Box::new(Literal::from("I like apples and bananas")),
                    Argument::Regex(regex::Regex::new("a").unwrap()),
                    "o",
                    Some(-1),
                ),
            ),
            (
                map![],
                Ok(Some("I like apples and bananas".into())),
                ReplaceFn::new(
                    Box::new(Literal::from("I like apples and bananas")),
                    Argument::Regex(regex::Regex::new("a").unwrap()),
                    "o",
                    Some(0),
                ),
            ),
            (
                map![],
                Ok(Some("I like opples and bananas".into())),
                ReplaceFn::new(
                    Box::new(Literal::from("I like apples and bananas")),
                    Argument::Regex(regex::Regex::new("a").unwrap()),
                    "o",
                    Some(1),
                ),
            ),
            (
                map![],
                Ok(Some("I like opples ond bananas".into())),
                ReplaceFn::new(
                    Box::new(Literal::from("I like apples and bananas")),
                    Argument::Regex(regex::Regex::new("a").unwrap()),
                    "o",
                    Some(2),
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
    fn check_replace_other() {
        let cases = vec![
            (
                map![],
                Ok(Some("I like biscuits and bananas".into())),
                ReplaceFn::new(
                    Box::new(Literal::from("I like apples and bananas")),
                    Argument::Expression(Box::new(Literal::from("apples"))),
                    "biscuits",
                    None,
                ),
            ),
            (
                map!["foo": "I like apples and bananas"],
                Ok(Some("I like opples and bananas".into())),
                ReplaceFn::new(
                    Box::new(Path::from("foo")),
                    Argument::Regex(regex::Regex::new("a").unwrap()),
                    "o",
                    Some(1),
                ),
            ),
            (
                map!["foo": "I like [apples] and bananas"],
                Ok(Some("I like biscuits and bananas".into())),
                ReplaceFn::new(
                    Box::new(Path::from("foo")),
                    Argument::Regex(regex::Regex::new("\\[apples\\]").unwrap()),
                    "biscuits",
                    None,
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
}
