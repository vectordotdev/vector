use crate::event;
use remap::prelude::*;
use std::collections::BTreeMap;
use std::iter::FromIterator;
use url::Url;

#[derive(Debug)]
pub struct ParseUrl;

impl Function for ParseUrl {
    fn identifier(&self) -> &'static str {
        "parse_url"
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

        Ok(Box::new(ParseUrlFn { value }))
    }
}

#[derive(Debug)]
struct ParseUrlFn {
    value: Box<dyn Expression>,
}

impl ParseUrlFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for ParseUrlFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let bytes = required!(state, object, self.value, Value::String(v) => v);

        Url::parse(&String::from_utf8_lossy(&bytes))
            .map_err(|e| format!("unable to parse url: {}", e).into())
            .map(event::Value::from)
            .map(Into::into)
            .map(Some)
    }
}

impl From<Url> for event::Value {
    fn from(url: Url) -> Self {
        let mut map = BTreeMap::<&str, event::Value>::new();

        map.insert("scheme", url.scheme().to_owned().into());
        map.insert("username", url.username().to_owned().into());
        map.insert(
            "password",
            url.password()
                .map(ToOwned::to_owned)
                .unwrap_or_default()
                .into(),
        );
        map.insert("path", url.path().to_owned().into());
        map.insert("host", url.host_str().map(ToOwned::to_owned).into());
        map.insert("port", url.port().map(|v| v as isize).into());
        map.insert("fragment", url.fragment().map(ToOwned::to_owned).into());
        map.insert(
            "query",
            url.query_pairs()
                .into_owned()
                .map(|(k, v)| (k, v.into()))
                .collect::<BTreeMap<String, event::Value>>()
                .into(),
        );

        event::Value::from_iter(map.into_iter().map(|(k, v)| (k.to_owned(), v)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn parse_url() {
        let cases = vec![
            (
                map![],
                Ok(Some(
                    map![
                            "scheme": "https",
                            "username": "",
                            "password": "",
                            "host": "vector.dev",
                            "port": Value::Null,
                            "path": "/",
                            "query": map![],
                            "fragment": Value::Null,
                    ]
                    .into(),
                )),
                ParseUrlFn::new(Box::new(Literal::from("https://vector.dev"))),
            ),
            (
                map![],
                Ok(Some(
                    map![
                            "scheme": "ftp",
                            "username": "foo",
                            "password": "bar",
                            "host": "vector.dev",
                            "port": 4343,
                            "path": "/foobar",
                            "query": map!["hello": "world"],
                            "fragment": "123",
                    ]
                    .into(),
                )),
                ParseUrlFn::new(Box::new(Literal::from(
                    "ftp://foo:bar@vector.dev:4343/foobar?hello=world#123",
                ))),
            ),
            (
                map![],
                Err("function call error: unable to parse url: relative URL without a base".into()),
                ParseUrlFn::new(Box::new(Literal::from("INVALID"))),
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
