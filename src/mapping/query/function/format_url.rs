use super::prelude::*;
use std::collections::BTreeMap;
use std::convert::TryFrom;
use url::Url;

const SCHEME: &str = "scheme";
const USERNAME: &str = "username";
const PASSWORD: &str = "password";
const HOST: &str = "host";
const PORT: &str = "port";
const PATH: &str = "path";
const FRAGMENT: &str = "fragment";
const QUERY: &str = "query";

#[derive(Debug)]
pub(in crate::mapping) struct FormatUrlFn {
    query: Box<dyn Function>,
}

impl FormatUrlFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

impl Function for FormatUrlFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let components = required!(ctx, self.query, Value::Map(v) => v);

        components_to_url(components).map(|url| url.into_string().into())
    }

    fn parameters() -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Map(_)),
            required: true,
        }]
    }
}

impl TryFrom<ArgumentList> for FormatUrlFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;

        Ok(Self { query })
    }
}

fn components_to_url(components: BTreeMap<String, Value>) -> Result<Url> {
    let scheme = components
        .get(SCHEME)
        .map(|v| string(SCHEME, v))
        .transpose()?
        .ok_or_else(|| missing(SCHEME))?;

    let username = components
        .get(USERNAME)
        .map(|v| string(USERNAME, v))
        .transpose()?
        .unwrap_or_else(|| "".to_owned());

    let password = components
        .get(PASSWORD)
        .and_then(Value::as_option)
        .map(|v| string(PASSWORD, v))
        .transpose()?;

    let host = components
        .get(HOST)
        .and_then(Value::as_option)
        .map(|v| string(HOST, v))
        .transpose()?;

    let port = components
        .get(PORT)
        .and_then(Value::as_option)
        .map(|v| {
            v.as_i64()
                .map(|i| i as u16)
                .ok_or_else(|| invalid(PORT, "map", v.kind()))
        })
        .transpose()?;

    let path = components
        .get(PATH)
        .map(|v| string(PATH, v))
        .transpose()?
        .unwrap_or_else(|| "".to_owned());

    let fragment = components
        .get(FRAGMENT)
        .and_then(Value::as_option)
        .map(|v| string(FRAGMENT, v))
        .transpose()?;

    let query = components
        .get(QUERY)
        .map(|v| v.as_map().ok_or_else(|| invalid(QUERY, "map", v.kind())))
        .transpose()?
        .map(|v| {
            v.iter()
                .map(|(k, v)| (k, v.to_string_lossy()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // This is a really awkward way to create a valid URL from components, but
    // it doesn't appear the `Url` API offers any better way to handle this.
    let mut url = Url::parse("https://vector.dev").expect("valid");

    url.set_scheme(&scheme).map_err(|_| unknown(SCHEME))?;
    url.set_host(host.as_deref()).map_err(|_| unknown(HOST))?;
    url.set_username(&username).map_err(|_| unknown(USERNAME))?;
    url.set_password(password.as_deref())
        .map_err(|_| unknown(PASSWORD))?;
    url.set_path(&path);
    url.set_port(port).map_err(|_| unknown(PORT))?;
    url.set_fragment(fragment.as_deref());

    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }

    Ok(url)
}

fn string(component: &'static str, value: &Value) -> Result<String> {
    value
        .as_bytes2()
        .ok_or_else(|| invalid(component, "string", value.kind()))
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

#[inline]
fn missing(s: &'static str) -> String {
    format!("missing url component '{}'", s)
}

#[inline]
fn invalid(component: &'static str, expected: &str, got: &str) -> String {
    format!(
        "invalid type for url component '{}' (expected {}, got {})",
        component, expected, got,
    )
}

#[inline]
fn unknown(component: &'static str) -> String {
    format!("invalid url component '{}'", component)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

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
    fn function() {
        let cases = vec![
            (
                Event::from(""),
                Ok("https://vector.dev/".into()),
                FormatUrlFn::new(Box::new(Literal::from(map![
                    "scheme": "https",
                    "username": "",
                    "password": Value::Null,
                    "host": "vector.dev",
                    "port": Value::Null,
                    "path": "/",
                    "query": map![],
                    "fragment": Value::Null,
                ]))),
            ),
            (
                Event::from(""),
                Ok("ftp://foo:bar@vector.dev:4343/foobar?hello=world#123".into()),
                FormatUrlFn::new(Box::new(Literal::from(map![
                    "scheme": "ftp",
                    "username": "foo",
                    "password": "bar",
                    "host": "vector.dev",
                    "port": 4343,
                    "path": "/foobar",
                    "query": map!["hello": "world"],
                    "fragment": "123",
                ]))),
            ),
            (
                Event::from(""),
                Ok("https://duckduckgo.com/".into()),
                FormatUrlFn::new(Box::new(Literal::from(map![
                    "scheme": "https",
                    "host": "duckduckgo.com",
                ]))),
            ),
            (
                Event::from(""),
                Err("missing url component 'scheme'".to_owned()),
                FormatUrlFn::new(Box::new(Literal::from(map![
                    "host": "duckduckgo.com",
                ]))),
            ),
            (
                Event::from(""),
                Err(
                    "invalid type for url component 'host' (expected string, got integer)"
                        .to_owned(),
                ),
                FormatUrlFn::new(Box::new(Literal::from(map![
                    "scheme": "https",
                    "host": 1,
                ]))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
