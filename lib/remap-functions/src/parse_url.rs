use remap::prelude::*;
use std::collections::BTreeMap;
use std::iter::FromIterator;
use url::Url;
use value::Kind;

#[derive(Clone, Copy, Debug)]
pub struct ParseUrl;

impl Function for ParseUrl {
    fn identifier(&self) -> &'static str {
        "parse_url"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(ParseUrlFn { value }))
    }
}

#[derive(Debug, Clone)]
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
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.value.execute(state, object)?;
        let string = value.try_bytes_utf8_lossy()?;

        Url::parse(&string)
            .map_err(|e| format!("unable to parse url: {}", e).into())
            .map(url_to_value)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .into_fallible(true) // URL parsing error
            .with_inner_type(inner_type_def())
            .with_constraint(value::Kind::Map)
    }
}

/// The type defs of the fields contained by the returned map.
fn inner_type_def() -> Option<InnerTypeDef> {
    Some(inner_type_def! ({
        "scheme": Kind::Bytes,
        "username": Kind::Bytes,
        "password": Kind::Bytes,
        "path": Kind::Bytes | Kind::Null,
        "host": Kind::Bytes,
        "port": Kind::Bytes,
        "fragment": Kind::Bytes | Kind::Null,
        "query": Kind::Map,
    }))
}

fn url_to_value(url: Url) -> Value {
    let mut map = BTreeMap::<&str, Value>::new();

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
    map.insert("port", url.port().map(|v| v as i64).into());
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

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

    remap::test_type_def![
        value_string {
            expr: |_| ParseUrlFn { value: Literal::from("foo").boxed() },
            def: TypeDef { fallible: true, kind: value::Kind::Map, inner_type_def: inner_type_def() },
        }

        value_optional {
            expr: |_| ParseUrlFn { value: Box::new(Noop) },
            def: TypeDef { fallible: true, kind: value::Kind::Map, inner_type_def: inner_type_def() },
        }
    ];

    #[test]
    fn parse_url() {
        let cases = vec![
            (
                btreemap! {},
                Ok(btreemap! {
                        "scheme" => "https",
                        "username" => "",
                        "password" => "",
                        "host" => "vector.dev",
                        "port" => Value::Null,
                        "path" => "/",
                        "query" => btreemap!{},
                        "fragment" => Value::Null,
                }
                .into()),
                ParseUrlFn::new(Box::new(Literal::from("https://vector.dev"))),
            ),
            (
                btreemap! {},
                Ok(btreemap! {
                        "scheme" => "ftp",
                        "username" => "foo",
                        "password" => "bar",
                        "host" => "vector.dev",
                        "port" => 4343,
                        "path" => "/foobar",
                        "query" => btreemap!{ "hello" => "world" },
                        "fragment" => "123",
                }
                .into()),
                ParseUrlFn::new(Box::new(Literal::from(
                    "ftp://foo:bar@vector.dev:4343/foobar?hello=world#123",
                ))),
            ),
            (
                btreemap! {},
                Err("function call error: unable to parse url: relative URL without a base".into()),
                ParseUrlFn::new(Box::new(Literal::from("INVALID"))),
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
