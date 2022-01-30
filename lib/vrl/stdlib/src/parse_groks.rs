use std::{collections::BTreeMap, fmt};

use datadog_grok::{
    parse_grok,
    parse_grok_rules::{self, GrokRule},
};
use vrl::{
    diagnostic::{Label, Span},
    prelude::*,
};

#[derive(Debug)]
pub enum Error {
    InvalidGrokPattern(datadog_grok::parse_grok_rules::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidGrokPattern(err) => write!(f, "{}", err),
        }
    }
}

impl std::error::Error for Error {}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        109
    }

    fn labels(&self) -> Vec<Label> {
        match self {
            Error::InvalidGrokPattern(err) => {
                vec![Label::primary(
                    format!("grok pattern error: {}", err),
                    Span::default(),
                )]
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ParseGroks;

impl Function for ParseGroks {
    fn identifier(&self) -> &'static str {
        "parse_groks"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "patterns",
                kind: kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "remove_empty",
                kind: kind::BOOLEAN,
                required: false,
            },
            Parameter {
                keyword: "aliases",
                kind: kind::OBJECT,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "parse grok pattern",
            source: indoc! {r#"
                parse_groks!(
                    "2020-10-02T23:22:12.223222Z info hello world",
                    patterns: [
                        "%{common_prefix} %{_status} %{_message}",
                        "%{common_prefix} %{_message}"
                    ],
                    aliases: {
                        "common_prefix": "%{_timestamp} %{_loglevel}",
                        "_timestamp": "%{TIMESTAMP_ISO8601:timestamp}",
                        "_loglevel": "%{LOGLEVEL:level}",
                        "_status": "%{POSINT:status}",
                        "_message": "%{GREEDYDATA:message}"
                    })
            "#},
            result: Ok(indoc! {r#"
                {
                    "timestamp": "2020-10-02T23:22:12.223222Z",
                    "level": "info",
                    "message": "hello world"
                }
            "#}),
        }]
    }

    fn compile_argument(
        &self,
        args: &[(&'static str, Option<FunctionArgument>)],
        _info: &FunctionCompileContext,
        name: &str,
        expr: Option<&expression::Expr>,
    ) -> CompiledArgument {
        match (name, expr) {
            ("patterns", Some(expr)) => {
                let aliases: Option<&FunctionArgument> = args.iter().find_map(|(name, arg)| {
                    if *name == "aliases" {
                        arg.as_ref()
                    } else {
                        None
                    }
                });

                let patterns = expr.as_value().unwrap();
                let patterns = patterns
                    .try_array()
                    .unwrap()
                    .into_iter()
                    .map(|value| {
                        let pattern = value
                            .try_bytes_utf8_lossy()
                            .expect("grok pattern not bytes")
                            .into_owned();
                        Ok(pattern)
                    })
                    .collect::<std::result::Result<Vec<String>, vrl::function::Error>>()?;

                let aliases = aliases
                .map(|aliases| {
                    aliases
                        .as_value()
                        .unwrap()
                        .try_object()
                        .unwrap()
                        .into_iter()
                        .map(|(key, expr)| {
                            let alias = expr
                                .try_bytes_utf8_lossy()
                                .expect("should be a string")
                                .into_owned();
                            Ok((key, alias))
                        })
                    .collect::<std::result::Result<BTreeMap<String, String>, vrl::function::Error>>().unwrap()
                })
                .unwrap_or_default();

                // We use a datadog library here because it is a superset of grok.
                let grok_rules =
                    parse_grok_rules::parse_grok_rules(&patterns, aliases).map_err(|e| {
                        Box::new(Error::InvalidGrokPattern(e)) as Box<dyn DiagnosticError>
                    })?;

                Ok(Some(Box::new(grok_rules) as _))
            }
            _ => Ok(None),
        }
    }

    fn call_by_vm(
        &self,
        _ctx: &mut Context,
        args: &mut VmArgumentList,
    ) -> std::result::Result<Value, ExpressionError> {
        let value = args.required("value");
        let bytes = value.try_bytes_utf8_lossy()?;

        let remove_empty = args
            .optional("remove_empty")
            .map(|v| v.as_boolean().unwrap_or(false))
            .unwrap_or(false);

        let grok_rules = args
            .required_any("patterns")
            .downcast_ref::<Vec<GrokRule>>()
            .unwrap();

        let v = parse_grok::parse_grok(bytes.as_ref(), grok_rules, remove_empty)
            .map_err(|e| format!("unable to parse grok: {}", e))?;

        Ok(v)
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        let patterns = arguments
            .required_array("patterns")?
            .into_iter()
            .map(|expr| {
                let pattern = expr
                    .as_value()
                    .ok_or(vrl::function::Error::ExpectedStaticExpression {
                        keyword: "patterns",
                        expr,
                    })?
                    .try_bytes_utf8_lossy()
                    .expect("grok pattern not bytes")
                    .into_owned();
                Ok(pattern)
            })
            .collect::<std::result::Result<Vec<String>, vrl::function::Error>>()?;

        let aliases = arguments
            .optional_object("aliases")?
            .unwrap_or_default()
            .into_iter()
            .map(|(key, expr)| {
                let alias = expr
                    .as_value()
                    .ok_or(vrl::function::Error::ExpectedStaticExpression {
                        keyword: "aliases",
                        expr,
                    })
                    .map(|e| {
                        e.try_bytes_utf8_lossy()
                            .expect("should be a string")
                            .into_owned()
                    })?;
                Ok((key, alias))
            })
            .collect::<std::result::Result<BTreeMap<String, String>, vrl::function::Error>>()?;

        // we use a datadog library here because it is a superset of grok
        let grok_rules = parse_grok_rules::parse_grok_rules(&patterns, aliases)
            .map_err(|e| Box::new(Error::InvalidGrokPattern(e)) as Box<dyn DiagnosticError>)?;

        let remove_empty = arguments
            .optional("remove_empty")
            .unwrap_or_else(|| expr!(false));

        Ok(Box::new(ParseGrokFn {
            value,
            grok_rules,
            remove_empty,
        }))
    }
}

#[derive(Clone, Debug)]
struct ParseGrokFn {
    value: Box<dyn Expression>,
    grok_rules: Vec<GrokRule>,
    remove_empty: Box<dyn Expression>,
}

impl Expression for ParseGrokFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let bytes = value.try_bytes_utf8_lossy()?;
        let remove_empty = self.remove_empty.resolve(ctx)?.try_boolean()?;

        let v = parse_grok::parse_grok(bytes.as_ref(), &self.grok_rules, remove_empty)
            .map_err(|e| format!("unable to parse grok: {}", e))?;

        Ok(v)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().object::<(), Kind>(map! {
            (): Kind::all(),
        })
    }
}

#[cfg(test)]
mod test {
    use shared::btreemap;

    use super::*;

    test_function![
        parse_grok => ParseGroks;

        invalid_grok {
            args: func_args![ value: "foo",
                              patterns: vec!["%{NOG}"]],
            want: Err("failed to parse grok expression '\\A%{NOG}\\z': The given pattern definition name \"NOG\" could not be found in the definition map"),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all(),
            }),
        }

        error {
            args: func_args![ value: "an ungrokkable message",
                              patterns: vec!["%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"]],
            want: Err("unable to parse grok: value does not match any rule"),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all(),
            }),
        }

        error2 {
            args: func_args![ value: "2020-10-02T23:22:12.223222Z an ungrokkable message",
                              patterns: vec!["%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"]],
            want: Err("unable to parse grok: value does not match any rule"),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all(),
            }),
        }

        parsed {
            args: func_args![ value: "2020-10-02T23:22:12.223222Z info Hello world",
                              patterns: vec!["%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"]],
            want: Ok(Value::from(btreemap! {
                "timestamp" => "2020-10-02T23:22:12.223222Z",
                "level" => "info",
                "message" => "Hello world",
            })),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all(),
            }),
        }

        parsed2 {
            args: func_args![ value: "2020-10-02T23:22:12.223222Z",
                              patterns: vec!["(%{TIMESTAMP_ISO8601:timestamp}|%{LOGLEVEL:level})"]],
            want: Ok(Value::from(btreemap! {
                "timestamp" => "2020-10-02T23:22:12.223222Z",
                "level" => "",
            })),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all(),
            }),
        }

        remove_empty {
            args: func_args![ value: "2020-10-02T23:22:12.223222Z",
                              patterns: vec!["(%{TIMESTAMP_ISO8601:timestamp}|%{LOGLEVEL:level})"],
                              remove_empty: true,
            ],
            want: Ok(Value::from(
                btreemap! { "timestamp" => "2020-10-02T23:22:12.223222Z" },
            )),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all(),
            }),
        }

        multiple_patterns_and_aliases_first_pattern_matches {
            args: func_args![
                value: r##"2020-10-02T23:22:12.223222Z info 200 hello world"##,
                patterns: Value::Array(vec![
                    "%{common_prefix} %{_status} %{_message}".into(),
                    "%{common_prefix} %{_message}".into(),
                    ]),
                aliases: value!({
                    "common_prefix": "%{_timestamp} %{_loglevel}",
                    "_timestamp": "%{TIMESTAMP_ISO8601:timestamp}",
                    "_loglevel": "%{LOGLEVEL:level}",
                    "_status": "%{POSINT:status}",
                    "_message": "%{GREEDYDATA:message}"
                })
            ],
            want: Ok(Value::from(btreemap! {
                "timestamp" => "2020-10-02T23:22:12.223222Z",
                "level" => "info",
                "status" => "200",
                "message" => "hello world"
            })),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all(),
            }),
        }

        multiple_patterns_and_aliases_second_pattern_matches {
            args: func_args![
                value: r##"2020-10-02T23:22:12.223222Z info hello world"##,
                patterns: Value::Array(vec![
                    "%{common_prefix} %{_status} %{_message}".into(),
                    "%{common_prefix} %{_message}".into(),
                    ]),
                aliases: value!({
                    "common_prefix": "%{_timestamp} %{_loglevel}",
                    "_timestamp": "%{TIMESTAMP_ISO8601:timestamp}",
                    "_loglevel": "%{LOGLEVEL:level}",
                    "_status": "%{POSINT:status}",
                    "_message": "%{GREEDYDATA:message}"
                })
            ],
            want: Ok(Value::from(btreemap! {
                "timestamp" => "2020-10-02T23:22:12.223222Z",
                "level" => "info",
                "message" => "hello world"
            })),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all(),
            }),
        }

        datadog_nginx {
            args: func_args![
                value: r##"127.0.0.1 - frank [13/Jul/2016:10:55:36] "GET /apache_pb.gif HTTP/1.0" 200 2326 0.202 "http://www.perdu.com/" "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/55.0.2883.87 Safari/537.36" "-""##,
                patterns: Value::Array(vec![
                    r#"%{access_common}"#.into(),
                    r#"%{access_common} (%{number:duration:scale(1000000000)} )?"%{_referer}" "%{_user_agent}"( "%{_x_forwarded_for}")?.*"#.into(),
                    ]),
                aliases: value!({
                    "access_common": r#"%{_client_ip} %{_ident} %{_auth} \[%{_date_access}\] "(?>%{_method} |)%{_url}(?> %{_version}|)" %{_status_code} (?>%{_bytes_written}|-)"#,
                    "_auth": r#"%{notSpace:http.auth:nullIf("-")}"#,
                    "_bytes_written": r#"%{integer:network.bytes_written}"#,
                    "_client_ip": r#"%{ipOrHost:network.client.ip}"#,
                    "_version": r#"HTTP\/%{regex("\\d+\\.\\d+"):http.version}"#,
                    "_url": r#"%{notSpace:http.url}"#,
                    "_ident": r#"%{notSpace:http.ident}"#,
                    "_user_agent": r#"%{regex("[^\\\"]*"):http.useragent}"#,
                    "_referer": r#"%{notSpace:http.referer}"#,
                    "_status_code": r#"%{integer:http.status_code}"#,
                    "_method": r#"%{word:http.method}"#,
                    "_date_access": r#"%{notSpace:date_access}"#,
                    "_x_forwarded_for": r#"%{regex("[^\\\"]*"):http._x_forwarded_for:nullIf("-")}"#
                })
            ],
            want: Ok(Value::Object(btreemap! {
                "date_access" => "13/Jul/2016:10:55:36",
                "duration" => 202000000,
                "http" => btreemap! {
                    "auth" => "frank",
                    "ident" => "-",
                    "method" => "GET",
                    "status_code" => 200,
                    "url" => "/apache_pb.gif",
                    "version" => "1.0",
                    "referer" => "http://www.perdu.com/",
                    "useragent" => "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/55.0.2883.87 Safari/537.36",
                    "_x_forwarded_for" => Value::Null,
                },
                "network" => btreemap! {
                    "bytes_written" => 2326,
                    "client" => btreemap! {
                        "ip" => "127.0.0.1"
                    }
                }
            })),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all(),
            }),
        }
    ];
}
