use remap::prelude::*;

#[derive(Debug)]
pub struct StripWhitespace;

impl Function for StripWhitespace {
    fn identifier(&self) -> &'static str {
        "strip_whitespace"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::String(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;

        Ok(Box::new(StripWhitespaceFn { value }))
    }
}

#[derive(Debug)]
struct StripWhitespaceFn {
    value: Box<dyn Expression>,
}

impl StripWhitespaceFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for StripWhitespaceFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let value = required!(state, object, self.value, Value::String(b) => String::from_utf8_lossy(&b).into_owned());

        Ok(Some(value.trim().into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn strip_whitespace() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": ""],
                Ok(Some("".into())),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": "     "],
                Ok(Some("".into())),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": "hi there"],
                Ok(Some("hi there".into())),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": "           hi there        "],
                Ok(Some("hi there".into())),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": " \u{3000}\u{205F}\u{202F}\u{A0}\u{9} ❤❤ hi there ❤❤  \u{9}\u{A0}\u{202F}\u{205F}\u{3000} "],
                Ok(Some("❤❤ hi there ❤❤".into())),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
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
