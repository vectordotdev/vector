use std::collections::BTreeMap;

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

    fn call_by_vm(&self, _ctx: &mut Context, arguments: &mut VmArgumentList) -> Resolved {
        let value = arguments.required("value");
        let string = value.try_bytes_utf8_lossy()?;
        let default_known_ports = arguments
            .optional("default_known_ports")
            .map(|val| val.as_boolean().unwrap_or(false))
            .unwrap_or(false);

        Url::parse(&string)
            .map_err(|e| format!("unable to parse url: {}", e).into())
            .map(|url| url_to_value(url, default_known_ports))
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let default_known_ports = arguments
            .optional("default_known_ports")
            .unwrap_or_else(|| expr!(false));

        Ok(Box::new(ParseUrlFn {
            value,
            default_known_ports,
        }))
    }
}

#[derive(Debug, Clone)]
struct ParseUrlFn {
    value: Box<dyn Expression>,
    default_known_ports: Box<dyn Expression>,
}

impl Expression for ParseUrlFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let string = value.try_bytes_utf8_lossy()?;

        let default_known_ports = self.default_known_ports.resolve(ctx)?.try_boolean()?;

        Url::parse(&string)
            .map_err(|e| format!("unable to parse url: {}", e).into())
            .map(|url| url_to_value(url, default_known_ports))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().object(type_def())
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

fn type_def() -> BTreeMap<&'static str, TypeDef> {
    map! {
        "scheme": Kind::Bytes,
        "username": Kind::Bytes,
        "password": Kind::Bytes,
        "path": Kind::Bytes | Kind::Null,
        "host": Kind::Bytes,
        "port": Kind::Integer | Kind::Null,
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
            tdef: TypeDef::new().fallible().object::<&'static str, TypeDef>(type_def()),
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
            tdef: TypeDef::new().fallible().object::<&'static str, TypeDef>(type_def()),
        }

        default_port {
            args: func_args![value: value!("https://vector.dev"), default_known_ports: true],
            want: Ok(value!({
                fragment: (),
                host: "vector.dev",
                password: "",
                path: "/",
                port: 443,
                query: {},
                scheme: "https",
                username: "",
            })),
            tdef: TypeDef::new().fallible().object::<&'static str, TypeDef>(type_def()),
        }
    ];
}
