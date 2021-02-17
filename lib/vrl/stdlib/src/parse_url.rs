use std::collections::BTreeMap;
use std::iter::FromIterator;
use url::Url;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ParseUrl;

impl Function for ParseUrl {
    fn identifier(&self) -> &'static str {
        "parse_url"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "parse url",
            source: r#"parse_url!("https://vector.dev")"#,
            result: Ok(indoc! {r#"
                {
                    "fragment": null,
                    "host": "vector.dev",
                    "password": "",
                    "path": "/",
                    "port": null,
                    "query": {},
                    "scheme": "https",
                    "username": ""
                }
            "#}),
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ParseUrlFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ParseUrlFn {
    value: Box<dyn Expression>,
}

impl Expression for ParseUrlFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let string = value.unwrap_bytes_utf8_lossy();

        Url::parse(&string)
            .map_err(|e| format!("unable to parse url: {}", e).into())
            .map(url_to_value)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().object(type_def())
    }
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

fn type_def() -> BTreeMap<&'static str, TypeDef> {
    map! {
        "scheme": Kind::Bytes,
        "username": Kind::Bytes,
        "password": Kind::Bytes,
        "path": Kind::Bytes | Kind::Null,
        "host": Kind::Bytes,
        "port": Kind::Bytes,
        "fragment": Kind::Bytes | Kind::Null,
        "query": TypeDef::new().object::<(), Kind>(map! {
            (): Kind::Bytes,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        parse_url => ParseUrl;

        type_def {
            args: func_args![value: value!("https://vector.dev")],
            want: Ok(value!({
                fragment: (),
                host: "vector.dev",
                password: "",
                path: "/",
                port: (),
                query: {},
                scheme: "https",
                username: "",
            })),
            tdef: TypeDef::new().fallible().object::<&'static str, TypeDef>(type_def()),
        }
    ];
}
