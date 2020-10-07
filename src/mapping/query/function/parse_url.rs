use super::prelude::*;
use std::collections::BTreeMap;
use std::iter::FromIterator;
use url::Url;

#[derive(Debug)]
pub(in crate::mapping) struct ParseUrlFn {
    query: Box<dyn Function>,
}

impl ParseUrlFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

impl Function for ParseUrlFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let bytes = required!(ctx, self.query, Value::Bytes(v) => v);

        Url::parse(&String::from_utf8_lossy(&bytes))
            .map_err(|e| format!("unable to parse url: {}", e))
            .map(Into::into)
    }

    fn parameters() -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }
}

impl TryFrom<ArgumentList> for ParseUrlFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;

        Ok(Self { query })
    }
}

impl From<Url> for Value {
    fn from(url: Url) -> Self {
        let mut map = BTreeMap::<&str, Value>::new();

        map.insert("scheme", url.scheme().to_owned().into());
        map.insert("username", url.username().to_owned().into());
        map.insert("path", url.path().to_owned().into());
        map.insert("password", url.password().map(ToOwned::to_owned).into());
        map.insert("host", url.host_str().map(ToOwned::to_owned).into());
        map.insert("port", url.port().map(|v| v as isize).into());
        map.insert("fragment", url.fragment().map(ToOwned::to_owned).into());
        map.insert(
            "query",
            url.query_pairs()
                .into_owned()
                .map(|(k, v)| (k, v.into()))
                .collect::<BTreeMap<String, Value>>()
                .into(),
        );

        Value::from_iter(map.into_iter().map(|(k, v)| (k.to_owned(), v)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! map {
        () => (
            Value::Map(BTreeMap::new())
        );
        ($($k:tt: $v:expr),+ $(,)?) => (
            Value::from(vec![
                $(($k.into(), $v.into())),+
            ].into_iter().collect::<BTreeMap<String, Value>>())
        );
    }

    #[test]
    fn parse_duration() {
        let cases = vec![
            (
                Event::from(""),
                Ok(map![
                    "scheme": "https",
                    "username": "",
                    "password": Value::Null,
                    "host": "vector.dev",
                    "port": Value::Null,
                    "path": "/",
                    "query": map![],
                    "fragment": Value::Null,
                ]),
                ParseUrlFn::new(Box::new(Literal::from("https://vector.dev"))),
            ),
            (
                Event::from(""),
                Ok(map![
                    "scheme": "ftp",
                    "username": "foo",
                    "password": "bar",
                    "host": "vector.dev",
                    "port": 4343,
                    "path": "/foobar",
                    "query": map!["hello": "world"],
                    "fragment": "123",
                ]),
                ParseUrlFn::new(Box::new(Literal::from(
                    "ftp://foo:bar@vector.dev:4343/foobar?hello=world#123",
                ))),
            ),
            (
                Event::from(""),
                Err("unable to parse url: relative URL without a base".to_owned()),
                ParseUrlFn::new(Box::new(Literal::from("INVALID"))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
