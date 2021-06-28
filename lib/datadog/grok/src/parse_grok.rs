use std::sync::Arc;

use grok::Grok;
use itertools::FoldWhile::{Continue, Done};
use itertools::Itertools;

use lookup::LookupBuf;
use percent_encoding::percent_decode;
use shared::btreemap;
use shared::conversion::Conversion;
use vector_core::event::Value;

use crate::ast as grok_ast;
use crate::ast::FunctionArgument;
use crate::grok_filter::apply_filter;
use crate::parse_grok_rules::Error as GrokStaticError;
use crate::parse_grok_rules::{parse_grok_rules, GrokRule};
use parsing::{query_string, ruby_hash};
use regex::Regex;
use std::collections::BTreeMap;
use std::convert::TryInto;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("failed to apply filter '{}'" , .0)]
    FailedToApplyFilter(String),
    #[error("value does not match any rule")]
    NoMatch,
}

pub fn parse_grok(source_field: &str, grok_rules: &Vec<GrokRule>) -> Result<Value, Error> {
    grok_rules
        .iter()
        .fold_while(Err(Error::NoMatch), |_, rule| {
            let result = apply_grok_rule(source_field, rule);
            if let Err(Error::NoMatch) = result {
                Continue(result)
            } else {
                Done(result)
            }
        })
        .into_inner()
}

fn apply_grok_rule(source: &str, grok_rule: &GrokRule) -> Result<Value, Error> {
    let mut parsed = Value::from(btreemap! {});

    if let Some(ref matches) = grok_rule.pattern.match_against(source) {
        for (name, value) in matches.iter() {
            let path: LookupBuf = name.parse().expect("path always should be valid");
            parsed.insert(path, Value::from(value));
        }

        // apply filters
        grok_rule.filters.iter().for_each(|(path, filters)| {
            filters.iter().for_each(|filter| {
                let result = apply_filter(
                    parsed
                        .get(path)
                        .expect("the field value is missing")
                        .unwrap(),
                    filter,
                );
                if result.is_ok() {
                    parsed.insert(path.to_owned(), result.unwrap());
                } else {
                    parsed.insert(path.to_owned(), Value::Null);
                }
            });
        });

        Ok(parsed)
    } else {
        Err(Error::NoMatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_grok() {
        let rules = parse_grok_rules(
            &vec![],
            &vec![
                "simple %{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
                    .to_string(),
            ],
        )
        .expect("should parse rules");
        let parsed = parse_grok("2020-10-02T23:22:12.223222Z info Hello world", &rules).unwrap();

        assert_eq!(
            parsed,
            Value::from(btreemap! {
                "timestamp" => "2020-10-02T23:22:12.223222Z",
                "level" => "info",
                "message" => "Hello world"
            })
        );
    }

    #[test]
    fn parses_complex_grok() {
        let rules = parse_grok_rules(
            // helper rules
            &vec![
                r#"_auth %{notSpace:http.auth:nullIf("-")}"#.to_string(),
                r#"_bytes_written %{integer:network.bytes_written}"#.to_string(),
                r#"_client_ip %{ipOrHost:network.client.ip}"#.to_string(),
                r#"_version HTTP\/(?<http.version>\d+\.\d+)"#.to_string(),
                r#"_url %{notSpace:http.url}"#.to_string(),
                r#"_ident %{notSpace:http.ident}"#.to_string(),
                r#"_user_agent %{regex("[^\\\"]*"):http.useragent}"#.to_string(),
                r#"_referer %{notSpace:http.referer}"#.to_string(),
                r#"_status_code %{integer:http.status_code}"#.to_string(),
                r#"_method %{word:http.method}"#.to_string(),
                r#"_date_access %{date("dd/MMM/yyyy:HH:mm:ss Z"):date_access}"#.to_string(),
                r#"_x_forwarded_for %{regex("[^\\\"]*"):http._x_forwarded_for:nullIf("-")}"#.to_string()],
            // parsing rules
            &vec![
                r#"access.common %{_client_ip} %{_ident} %{_auth} \[%{_date_access}\] "(?>%{_method} |)%{_url}(?> %{_version}|)" %{_status_code} (?>%{_bytes_written}|-)"#.to_string(),
                r#"access.combined %{access.common} (%{number:duration:scale(1000000000)} )?"%{_referer}" "%{_user_agent}"( "%{_x_forwarded_for}")?.*"#.to_string()
            ]).expect("should parse rules");
        let parsed = parse_grok(r##"127.0.0.1 - frank [13/Jul/2016:10:55:36 +0000] "GET /apache_pb.gif HTTP/1.0" 200 2326 0.202 "http://www.perdu.com/" "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/55.0.2883.87 Safari/537.36" "-""##, &rules).unwrap();

        assert_eq!(
            parsed,
            Value::from(btreemap! {
                "date_access" => "13/Jul/2016:10:55:36 +0000",
                "duration" => 202000000.0,
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
            })
        );
    }

    #[test]
    fn supports_matchers() {
        test_grok_pattern(vec![
            (
                "%{numberStr:field}",
                "-1.2",
                Ok(Value::Bytes("-1.2".into())),
            ),
            ("%{number:field}", "-1.2", Ok(Value::Float(-1.2_f64))),
            ("%{number:field}", "-1", Ok(Value::Float(-1_f64))),
            (
                "%{numberExt:field}",
                "-1234e+3",
                Ok(Value::Float(-1234e+3_f64)),
            ),
            ("%{numberExt:field}", ".1e+3", Ok(Value::Float(0.1e+3_f64))),
            ("%{integer:field}", "-2", Ok(Value::Integer(-2))),
            ("%{integerExt:field}", "+2", Ok(Value::Integer(2))),
            ("%{integerExt:field}", "-2", Ok(Value::Integer(-2))),
            ("%{integerExt:field}", "-1e+2", Ok(Value::Integer(-100))),
            ("%{integerExt:field}", "1234.1e+5", Err(Error::NoMatch)),
            ("%{boolean:field}", "tRue", Ok(Value::Boolean(true))), // true/false are default values(case-insensitive)
            ("%{boolean:field}", "False", Ok(Value::Boolean(false))),
            (
                r#"%{boolean("ok", "no"):field}"#,
                "ok",
                Ok(Value::Boolean(true)),
            ),
            (
                r#"%{boolean("ok", "no"):field}"#,
                "no",
                Ok(Value::Boolean(false)),
            ),
            (r#"%{boolean("ok", "no"):field}"#, "No", Err(Error::NoMatch)),
            (
                "%{doubleQuotedString:field}",
                r#""test  ""#,
                Ok(Value::Bytes(r#""test  ""#.into())),
            ),
        ]);
    }

    #[test]
    fn supports_filters() {
        test_grok_pattern(vec![
            ("%{data:field:number}", "1.0", Ok(Value::Float(1.0_f64))),
            ("%{data:field:integer}", "1", Ok(Value::Integer(1))),
            (r#"%{data:field:nullIf("-")}"#, "-", Ok(Value::Null)),
            (
                r#"%{data:field:nullIf("-")}"#,
                "abc",
                Ok(Value::Bytes("abc".into())),
            ),
            ("%{data:field:boolean}", "tRue", Ok(Value::Boolean(true))),
            ("%{data:field:boolean}", "false", Ok(Value::Boolean(false))),
            (
                "%{data:field:json}",
                r#"{ "bool_field": true, "array": ["abc"] }"#,
                Ok(Value::from(
                    btreemap! { "bool_field" => Value::Boolean(true), "array" => Value::Array(vec!["abc".into()])},
                )),
            ),
            (
                "%{data:field:rubyhash}",
                r#"{ "test" => "value", "testNum" => 0.2, "testObj" => { "testBool" => true } }"#,
                Ok(Value::from(
                    btreemap! { "test" => "value", "testNum" => 0.2, "testObj" => Value::from(btreemap! {"testBool" => true})},
                )),
            ),
            (
                "%{data:field:querystring}",
                "?productId=superproduct&promotionCode=superpromo",
                Ok(Value::from(
                    btreemap! { "productId" => "superproduct", "promotionCode" => "superpromo"},
                )),
            ),
            (
                "%{data:field:lowercase}",
                "aBC",
                Ok(Value::Bytes("abc".into())),
            ),
            (
                "%{data:field:uppercase}",
                "Abc",
                Ok(Value::Bytes("ABC".into())),
            ),
            (
                "%{data:field:decodeuricomponent}",
                "%2Fservice%2Ftest",
                Ok(Value::Bytes("/service/test".into())),
            ),
            ("%{integer:field:scale(10)}", "1", Ok(Value::Float(10.0))),
            ("%{number:field:scale(0.5)}", "10.0", Ok(Value::Float(5.0))),
        ]);
    }

    fn test_grok_pattern(tests: Vec<(&str, &str, Result<Value, Error>)>) {
        for (filter, k, v) in tests {
            let rules = parse_grok_rules(&vec![], &vec![format!(r#"test {}"#, filter)])
                .expect("should parse rules");
            let parsed = parse_grok(k, &rules);

            if v.is_ok() {
                assert_eq!(
                    parsed.unwrap().get("field").unwrap().unwrap().to_owned(),
                    v.unwrap()
                );
            } else {
                assert_eq!(parsed, v);
            }
        }
    }

    #[test]
    fn fails_on_invalid_grok_format() {
        assert_eq!(
            parse_grok_rules(&vec![], &vec!["%{data}".to_string()])
                .unwrap_err()
                .to_string(),
            "failed to parse grok expression '%{data}': format must be: 'ruleName definition'"
        );
    }

    #[test]
    fn fails_on_unknown_pattern_definition() {
        assert_eq!(
            parse_grok_rules(&vec![], &vec!["test %{unknown}".to_string()])
                .unwrap_err()
                .to_string(),
            r#"failed to parse grok expression '^%{unknown}$': The given pattern definition name "unknown" could not be found in the definition map"#
        );
    }

    #[test]
    fn fails_on_unknown_filter() {
        assert_eq!(
            parse_grok_rules(
                &vec![],
                &vec!["test %{data:field:unknownFilter}".to_string()]
            )
            .unwrap_err()
            .to_string(),
            r#"unknown filter 'unknownFilter'"#
        );
    }

    #[test]
    fn fails_on_invalid_matcher_parameter() {
        assert_eq!(
            parse_grok_rules(&vec![], &vec!["test_rule %{regex(1):field}".to_string()])
                .unwrap_err()
                .to_string(),
            r#"invalid arguments for the function 'regex'"#
        );
    }

    #[test]
    fn fails_on_invalid_filter_parameter() {
        assert_eq!(
            parse_grok_rules(
                &vec![],
                &vec!["test_rule %{data:field:scale()}".to_string()]
            )
            .unwrap_err()
            .to_string(),
            r#"invalid arguments for the function 'scale'"#
        );
    }

    #[test]
    fn sets_field_to_null_on_filter_runtime_error() {
        let rules = parse_grok_rules(&vec![], &vec!["test_rule %{data:field:number}".to_string()])
            .expect("should parse rules");
        let parsed = parse_grok("not a number", &rules).unwrap();

        assert_eq!(
            parsed,
            Value::from(btreemap! {
                "field" => Value::Null,
            })
        );
    }

    #[test]
    fn fails_on_no_match() {
        let rules = parse_grok_rules(
            &vec![],
            &vec![
                "test_rule %{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
                    .to_string(),
            ],
        )
        .expect("should parse rules");
        let error = parse_grok("an ungrokkable message", &rules).unwrap_err();

        assert_eq!(error, Error::NoMatch);
    }
}
