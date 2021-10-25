use itertools::{
    FoldWhile::{Continue, Done},
    Itertools,
};

use lookup::LookupBuf;
use shared::btreemap;

use crate::{grok_filter::apply_filter, insert_field::insert_field, parse_grok_rules::GrokRule};
use tracing::warn;
use vrl_compiler::Value;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("failed to apply filter '{}' to '{}'", .0, .1)]
    FailedToApplyFilter(String, String),
    #[error("value does not match any rule")]
    NoMatch,
}

pub fn parse_grok(source_field: &str, grok_rules: &[GrokRule]) -> Result<Value, Error> {
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
            let path: LookupBuf = if name == "." {
                LookupBuf::root()
            } else {
                name.parse().expect("path should always be valid")
            };

            let mut value = Some(Value::from(value));

            // apply filters
            if let Some(filters) = grok_rule.filters.get(&path) {
                filters.iter().for_each(|filter| {
                    match apply_filter(&value.as_ref().unwrap(), filter) {
                        Ok(v) => value = Some(v),
                        Err(error) => {
                            warn!(message = "Error applying filter", path = %path, filter = %filter, %error);
                            value = None;
                        }
                    }
                });
            };

            if let Some(value) = value {
                insert_field(&mut parsed, path.clone(), value).unwrap_or_else(
                    |error| warn!(message = "Error updating field value", path = %path, %error),
                );
            }
        }

        Ok(parsed)
    } else {
        Err(Error::NoMatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_grok_rules::parse_grok_rules;
    use vrl_compiler::Value;

    #[test]
    fn parses_simple_grok() {
        let rules = parse_grok_rules(
            &[],
            &[
                "simple %{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
                    .to_string(),
            ],
        )
        .expect("couldn't parse rules");
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
            &[
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
                r#"_date_access %{notSpace:date_access}"#.to_string(),
                r#"_x_forwarded_for %{regex("[^\\\"]*"):http._x_forwarded_for:nullIf("-")}"#.to_string()],
            // parsing rules
            &[
                r#"access.common %{_client_ip} %{_ident} %{_auth} \[%{_date_access}\] "(?>%{_method} |)%{_url}(?> %{_version}|)" %{_status_code} (?>%{_bytes_written}|-)"#.to_string(),
                r#"access.combined %{access.common} (%{number:duration:scale(1000000000)} )?"%{_referer}" "%{_user_agent}"( "%{_x_forwarded_for}")?.*"#.to_string()
            ]).expect("couldn't parse rules");
        let parsed = parse_grok(r##"127.0.0.1 - frank [13/Jul/2016:10:55:36] "GET /apache_pb.gif HTTP/1.0" 200 2326 0.202 "http://www.perdu.com/" "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/55.0.2883.87 Safari/537.36" "-""##, &rules).unwrap();

        assert_eq!(
            parsed,
            Value::from(btreemap! {
                "date_access" => "13/Jul/2016:10:55:36",
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
            ("%{number:field}", "-1.2", Ok(Value::from(-1.2_f64))),
            ("%{number:field}", "-1", Ok(Value::from(-1_f64))),
            (
                "%{numberExt:field}",
                "-1234e+3",
                Ok(Value::from(-1234e+3_f64)),
            ),
            ("%{numberExt:field}", ".1e+3", Ok(Value::from(0.1e+3_f64))),
            ("%{integer:field}", "-2", Ok(Value::from(-2))),
            ("%{integerExt:field}", "+2", Ok(Value::from(2))),
            ("%{integerExt:field}", "-2", Ok(Value::from(-2))),
            ("%{integerExt:field}", "-1e+2", Ok(Value::from(-100))),
            ("%{integerExt:field}", "1234.1e+5", Err(Error::NoMatch)),
        ]);
    }

    #[test]
    fn supports_filters() {
        test_grok_pattern(vec![
            ("%{data:field:number}", "1.0", Ok(Value::from(1.0_f64))),
            ("%{data:field:integer}", "1", Ok(Value::from(1))),
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
            ("%{integer:field:scale(10)}", "1", Ok(Value::from(10.0))),
            ("%{number:field:scale(0.5)}", "10.0", Ok(Value::from(5.0))),
        ]);
    }

    fn test_grok_pattern_without_field(tests: Vec<(&str, &str, Result<Value, Error>)>) {
        for (filter, k, v) in tests {
            let rules = parse_grok_rules(&[], &[format!(r#"test {}"#, filter)])
                .expect("should parse rules");
            let parsed = parse_grok(k, &rules);

            if v.is_ok() {
                assert_eq!(parsed.unwrap(), v.unwrap());
            } else {
                assert_eq!(parsed, v);
            }
        }
    }

    fn test_grok_pattern(tests: Vec<(&str, &str, Result<Value, Error>)>) {
        for (filter, k, v) in tests {
            let rules = parse_grok_rules(&[], &[format!(r#"test {}"#, filter)])
                .expect("couldn't parse rules");
            let parsed = parse_grok(k, &rules);

            if v.is_ok() {
                assert_eq!(
                    parsed.unwrap(),
                    Value::from(btreemap! {
                        "field" =>  v.unwrap(),
                    })
                );
            } else {
                assert_eq!(parsed, v);
            }
        }
    }

    #[test]
    fn fails_on_invalid_grok_format() {
        assert_eq!(
            parse_grok_rules(&[], &["%{data}".to_string()])
                .unwrap_err()
                .to_string(),
            "failed to parse grok expression '%{data}': format must be: 'ruleName definition'"
        );
    }

    #[test]
    fn fails_on_unknown_pattern_definition() {
        assert_eq!(
            parse_grok_rules(&[], &["test %{unknown}".to_string()])
                .unwrap_err()
                .to_string(),
            r#"failed to parse grok expression '^%{unknown}$': The given pattern definition name "unknown" could not be found in the definition map"#
        );
    }

    #[test]
    fn fails_on_unknown_filter() {
        assert_eq!(
            parse_grok_rules(&[], &["test %{data:field:unknownFilter}".to_string()])
                .unwrap_err()
                .to_string(),
            r#"unknown filter 'unknownFilter'"#
        );
    }

    #[test]
    fn fails_on_invalid_matcher_parameter() {
        assert_eq!(
            parse_grok_rules(&[], &["test_rule %{regex(1):field}".to_string()])
                .unwrap_err()
                .to_string(),
            r#"invalid arguments for the function 'regex'"#
        );
    }

    #[test]
    fn fails_on_invalid_filter_parameter() {
        assert_eq!(
            parse_grok_rules(&[], &["test_rule %{data:field:scale()}".to_string()])
                .unwrap_err()
                .to_string(),
            r#"invalid arguments for the function 'scale'"#
        );
    }

    #[test]
    fn supports_filters_without_fields() {
        // if the value, after filters applied, is a map then merge it at the root level
        test_grok_pattern_without_field(vec![(
            "%{notSpace:standalone_field} '%{data::json}' '%{data::json}'",
            r#"value1 '{ "json_field1": "value2" }' '{ "json_field2": "value3" }'"#,
            Ok(Value::from(btreemap! {
                "standalone_field" => Value::Bytes("value1".into()),
                "json_field1" => Value::Bytes("value2".into()),
                "json_field2" => Value::Bytes("value3".into())
            })),
        )]);
        test_grok_pattern_without_field(vec![(
            "%{notSpace:standalone_field} '%{data:nested.json:json}' '%{data:nested.json:json}'",
            r#"value1 '{ "json_field1": "value2" }' '{ "json_field2": "value3" }'"#,
            Ok(Value::from(btreemap! {
                "standalone_field" => Value::Bytes("value1".into()),
                "nested" => btreemap! {
                    "json" =>  btreemap! {
                        "json_field1" => Value::Bytes("value2".into()),
                        "json_field2" => Value::Bytes("value3".into())
                    }
                }
            })),
        )]);
        // otherwise ignore it
        test_grok_pattern_without_field(vec![(
            "%{notSpace:standalone_field} %{data::integer}",
            r#"value1 1"#,
            Ok(Value::from(btreemap! {
                "standalone_field" => Value::Bytes("value1".into()),
            })),
        )]);
        // empty map if fails
        test_grok_pattern_without_field(vec![(
            "%{data::json}",
            r#"not a json"#,
            Ok(Value::from(btreemap! {})),
        )]);
    }

    #[test]
    fn ignores_field_if_filter_fails() {
        // empty map for filters like json
        test_grok_pattern_without_field(vec![(
            "%{notSpace:field1:integer} %{data:field2:json}",
            r#"not_a_number not a json"#,
            Ok(Value::from(btreemap! {})),
        )]);
    }

    #[test]
    fn fails_on_no_match() {
        let rules = parse_grok_rules(
            &[],
            &[
                "test_rule %{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
                    .to_string(),
            ],
        )
        .expect("couldn't parse rules");
        let error = parse_grok("an ungrokkable message", &rules).unwrap_err();

        assert_eq!(error, Error::NoMatch);
    }

    #[test]
    fn appends_to_the_same_field() {
        let rules = parse_grok_rules(
            &[],
            &[
                r#"simple %{integer:nested.field} %{notSpace:nested.field:uppercase} %{notSpace:nested.field:nullIf("-")}"#
                    .to_string(),
            ],
        )
            .expect("couldn't parse rules");
        let parsed = parse_grok("1 info -", &rules).unwrap();

        assert_eq!(
            parsed,
            Value::from(btreemap! {
                "nested" => btreemap! {
                   "field" =>  Value::Array(vec![1.into(), "INFO".into(), Value::Null]),
                },
            })
        );
    }
}
