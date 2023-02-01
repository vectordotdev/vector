use std::collections::BTreeMap;

use ::value::Value;
use url::Url;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ParseUrl;

impl Function for ParseUrl {
    fn identifier(&self) -> &'static str {
        "parse_url"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "default_known_ports",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
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
            },
            Example {
                title: "parse url with default ports",
                source: r#"parse_url!("https://vector.dev", default_known_ports: true)"#,
                result: Ok(indoc! {r#"
                {
                    "fragment": null,
                    "host": "vector.dev",
                    "password": "",
                    "path": "/",
                    "port": 443,
                    "query": {},
                    "scheme": "https",
                    "username": ""
                }
            "#}),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let default_known_ports = arguments
            .optional("default_known_ports")
            .unwrap_or_else(|| expr!(false));

        Ok(ParseUrlFn {
            value,
            default_known_ports,
        }
        .as_expr())
    }
}

#[derive(Debug, Clone)]
struct ParseUrlFn {
    value: Box<dyn Expression>,
    default_known_ports: Box<dyn Expression>,
}

impl FunctionExpression for ParseUrlFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let string = value.try_bytes_utf8_lossy()?;

        let default_known_ports = self.default_known_ports.resolve(ctx)?.try_boolean()?;

        Url::parse(&string)
            .map_err(|e| format!("unable to parse url: {e}").into())
            .map(|url| url_to_value(url, default_known_ports))
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::object(inner_kind()).fallible()
    }
}

fn url_to_value(url: Url, default_known_ports: bool) -> Value {
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

    let port = if default_known_ports {
        url.port_or_known_default()
    } else {
        url.port()
    };
    map.insert("port", port.into());
    map.insert("fragment", url.fragment().map(ToOwned::to_owned).into());
    map.insert(
        "query",
        url.query_pairs()
            .into_owned()
            .map(|(k, v)| (k, v.into()))
            .collect::<BTreeMap<String, Value>>()
            .into(),
    );

    map.into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect::<Value>()
}

fn inner_kind() -> BTreeMap<Field, Kind> {
    BTreeMap::from([
        ("scheme".into(), Kind::bytes()),
        ("username".into(), Kind::bytes()),
        ("password".into(), Kind::bytes()),
        ("path".into(), Kind::bytes().or_null()),
        ("host".into(), Kind::bytes()),
        ("port".into(), Kind::integer().or_null()),
        ("fragment".into(), Kind::bytes().or_null()),
        (
            "query".into(),
            Kind::object(Collection::from_unknown(Kind::bytes())),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        parse_url => ParseUrl;

        https {
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
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        default_port_specified {
            args: func_args![value: value!("https://vector.dev:443")],
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
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        default_port {
            args: func_args![value: value!("https://vector.dev"), default_known_ports: true],
            want: Ok(value!({
                fragment: (),
                host: "vector.dev",
                password: "",
                path: "/",
                port: 443_i64,
                query: {},
                scheme: "https",
                username: "",
            })),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }
    ];
}
