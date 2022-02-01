use crate::{
    grok_filter::apply_filter,
    parse_grok_rules::{GrokField, GrokRule},
};
use itertools::{
    FoldWhile::{Continue, Done},
    Itertools,
};
use tracing::warn;
use vector_common::btreemap;
use vrl_compiler::{Target, Value};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("failed to apply filter '{}' to '{}'", .0, .1)]
    FailedToApplyFilter(String, String),
    #[error("value does not match any rule")]
    NoMatch,
}

/// Parses a given source field value by applying the list of grok rules until the first match found.
pub fn parse_grok(
    source_field: &str,
    grok_rules: &[GrokRule],
    remove_empty: bool,
) -> Result<Value, Error> {
    grok_rules
        .iter()
        .fold_while(Err(Error::NoMatch), |_, rule| {
            let result = apply_grok_rule(source_field, rule, remove_empty);
            if let Err(Error::NoMatch) = result {
                Continue(result)
            } else {
                Done(result)
            }
        })
        .into_inner()
}

/// Tries to parse a given string with a given grok rule.
/// Returns a result value or an error otherwise.
/// Possible errors:
/// - FailedToApplyFilter - matches the rule, but there was a runtime error while applying on of the filters
/// - NoMatch - this rule does not match a given string
fn apply_grok_rule(source: &str, grok_rule: &GrokRule, remove_empty: bool) -> Result<Value, Error> {
    let mut parsed = Value::from(btreemap! {});

    if let Some(ref matches) = grok_rule.pattern.match_against(source) {
        for (name, value) in matches.iter() {
            let mut value = Some(Value::from(value));

            if let Some(GrokField {
                lookup: field,
                filters,
            }) = grok_rule.fields.get(name)
            {
                filters.iter().for_each(|filter| {
                    if let Some(ref v) = value {
                        match apply_filter(v, filter) {
                            Ok(v) => value = Some(v),
                            Err(error) => {
                                warn!(message = "Error applying filter", field = %field, filter = %filter, %error);
                                value = None;
                            }
                        }
                    }
                });

                if let Some(value) = value {
                    match value {
                        // root-level maps must be merged
                        Value::Object(map) if field.is_root() => {
                            parsed.as_object_mut().expect("root is object").extend(map);
                        }
                        // anything else at the root leve must be ignored
                        _ if field.is_root() => {}
                        // ignore empty strings if necessary
                        Value::Bytes(b) if remove_empty && b.is_empty() => {}
                        // otherwise just apply VRL lookup insert logic
                        _ => match parsed.get(field).expect("field does not exist") {
                            Some(Value::Array(mut values)) => values.push(value),
                            Some(v) => {
                                parsed.insert(field, Value::Array(vec![v, value])).unwrap_or_else(
                                    |error| warn!(message = "Error updating field value", field = %field, %error)
                                );
                            }
                            None => {
                                parsed.insert(field, value).unwrap_or_else(
                                    |error| warn!(message = "Error updating field value", field = %field, %error)
                                );
                            }
                        },
                    };
                }
            } else {
                // this must be a regex named capturing group (?<name>group),
                // where name can only be alphanumeric - thus we do not need to parse field names(no nested fields)
                parsed
                    .as_object_mut()
                    .expect("parsed value is not an object")
                    .insert(name.to_string(), value.into());
            }
        }

        Ok(parsed)
    } else {
        Err(Error::NoMatch)
    }
}

#[cfg(test)]
mod tests {
    use ordered_float::NotNan;
    use vrl_compiler::Value;

    use super::*;
    use crate::parse_grok_rules::parse_grok_rules;

    #[test]
    fn parses_simple_grok() {
        let rules = parse_grok_rules(
            &[
                "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
                    .to_string(),
            ],
            btreemap! {},
        )
        .expect("couldn't parse rules");
        let parsed = parse_grok(
            "2020-10-02T23:22:12.223222Z info Hello world",
            &rules,
            false,
        )
        .unwrap();

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
            // patterns
            &[
                r#"%{access.common}"#.to_string(),
                r#"%{access.common} (%{number:duration:scale(1000000000)} )?"%{_referer}" "%{_user_agent}"( "%{_x_forwarded_for}")?.*"#.to_string()
            ],
            // aliases
            btreemap! {
                "access.common" => r#"%{_client_ip} %{_ident} %{_auth} \[%{_date_access}\] "(?>%{_method} |)%{_url}(?> %{_version}|)" %{_status_code} (?>%{_bytes_written}|-)"#.to_string(),
                "_auth" => r#"%{notSpace:http.auth:nullIf("-")}"#.to_string(),
                "_bytes_written" => r#"%{integer:network.bytes_written}"#.to_string(),
                "_client_ip" => r#"%{ipOrHost:network.client.ip}"#.to_string(),
                "_version" => r#"HTTP\/%{regex("\\d+\\.\\d+"):http.version}"#.to_string(),
                "_url" => r#"%{notSpace:http.url}"#.to_string(),
                "_ident" => r#"%{notSpace:http.ident}"#.to_string(),
                "_user_agent" => r#"%{regex("[^\\\"]*"):http.useragent}"#.to_string(),
                "_referer" => r#"%{notSpace:http.referer}"#.to_string(),
                "_status_code" => r#"%{integer:http.status_code}"#.to_string(),
                "_method" => r#"%{word:http.method}"#.to_string(),
                "_date_access" => r#"%{notSpace:date_access}"#.to_string(),
                "_x_forwarded_for" => r#"%{regex("[^\\\"]*"):http._x_forwarded_for:nullIf("-")}"#.to_string()}).expect("couldn't parse rules");
        let parsed = parse_grok(r##"127.0.0.1 - frank [13/Jul/2016:10:55:36] "GET /apache_pb.gif HTTP/1.0" 200 2326 0.202 "http://www.perdu.com/" "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/55.0.2883.87 Safari/537.36" "-""##, &rules, false).unwrap();

        assert_eq!(
            parsed,
            Value::from(btreemap! {
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
            })
        );
    }

    #[test]
    fn supports_matchers() {
        test_grok_pattern(vec![
            ("%{number:field}", "-1.2", Ok(Value::from(-1.2_f64))),
            ("%{number:field}", "-1", Ok(Value::from(-1))),
            ("%{numberExt:field}", "-1234e+3", Ok(Value::from(-1234000))),
            ("%{numberExt:field}", ".1e+3", Ok(Value::from(100))),
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
            ("%{data:field:number}", "1.0", Ok(Value::from(1))),
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
            ("%{integer:field:scale(10)}", "1", Ok(Value::from(10))),
            ("%{number:field:scale(0.5)}", "10.0", Ok(Value::from(5))),
        ]);
    }

    fn test_grok_pattern(tests: Vec<(&str, &str, Result<Value, Error>)>) {
        for (filter, k, v) in tests {
            let rules = parse_grok_rules(&[filter.to_string()], btreemap! {})
                .expect("couldn't parse rules");
            let parsed = parse_grok(k, &rules, false);

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

    fn test_full_grok(tests: Vec<(&str, &str, Result<Value, Error>)>) {
        for (filter, k, v) in tests {
            let rules = parse_grok_rules(&[filter.to_string()], btreemap! {})
                .expect("couldn't parse rules");
            let parsed = parse_grok(k, &rules, false);

            assert_eq!(parsed, v);
        }
    }

    #[test]
    fn fails_on_unknown_pattern_definition() {
        assert_eq!(
            parse_grok_rules(&["%{unknown}".to_string()], btreemap! {})
                .unwrap_err()
                .to_string(),
            r#"failed to parse grok expression '\A%{unknown}\z': The given pattern definition name "unknown" could not be found in the definition map"#
        );
    }

    #[test]
    fn fails_on_unknown_filter() {
        assert_eq!(
            parse_grok_rules(&["%{data:field:unknownFilter}".to_string()], btreemap! {})
                .unwrap_err()
                .to_string(),
            r#"unknown filter 'unknownFilter'"#
        );
    }

    #[test]
    fn fails_on_invalid_matcher_parameter() {
        assert_eq!(
            parse_grok_rules(&["%{regex(1):field}".to_string()], btreemap! {})
                .unwrap_err()
                .to_string(),
            r#"invalid arguments for the function 'regex'"#
        );
    }

    #[test]
    fn fails_on_invalid_filter_parameter() {
        assert_eq!(
            parse_grok_rules(&["%{data:field:scale()}".to_string()], btreemap! {})
                .unwrap_err()
                .to_string(),
            r#"invalid arguments for the function 'scale'"#
        );
    }

    #[test]
    fn regex_with_empty_field() {
        test_grok_pattern(vec![(
            r#"%{regex("\\d+\\.\\d+")} %{data:field}"#,
            "1.0 field_value",
            Ok(Value::from("field_value")),
        )]);
    }

    #[test]
    fn does_not_merge_field_maps() {
        // only root-level maps are merged
        test_full_grok(vec![(
            "'%{data:nested.json:json}' '%{data:nested.json:json}'",
            r#"'{ "json_field1": "value2" }' '{ "json_field2": "value3" }'"#,
            Ok(Value::from(btreemap! {
                "nested" => btreemap! {
                    "json" =>  Value::Array(vec! [
                        Value::from(btreemap! { "json_field1" => Value::Bytes("value2".into()) }),
                        Value::from(btreemap! { "json_field2" => Value::Bytes("value3".into()) }),
                    ]),
                }
            })),
        )]);
    }

    #[test]
    fn supports_filters_without_fields() {
        // if the root-level value, after filters applied, is a map then merge it at the root level,
        // otherwise ignore it
        test_full_grok(vec![
            (
                "%{data::json}",
                r#"{ "json_field1": "value2" }"#,
                Ok(Value::from(btreemap! {
                    "json_field1" => Value::Bytes("value2".into()),
                })),
            ),
            (
                "%{notSpace:standalone_field} '%{data::json}' '%{data::json}' %{number::number}",
                r#"value1 '{ "json_field1": "value2" }' '{ "json_field2": "value3" }' 3"#,
                Ok(Value::from(btreemap! {
                    "standalone_field" => Value::Bytes("value1".into()),
                    "json_field1" => Value::Bytes("value2".into()),
                    "json_field2" => Value::Bytes("value3".into())
                })),
            ),
            // ignore non-map root-level fields
            (
                "%{notSpace:standalone_field} %{data::integer}",
                r#"value1 1"#,
                Ok(Value::from(btreemap! {
                    "standalone_field" => Value::Bytes("value1".into()),
                })),
            ),
            // empty map if fails
            (
                "%{data::json}",
                r#"not a json"#,
                Ok(Value::from(btreemap! {})),
            ),
        ]);
    }

    #[test]
    fn ignores_field_if_filter_fails() {
        // empty map for filters like json
        test_full_grok(vec![(
            "%{notSpace:field1:integer} %{data:field2:json}",
            r#"not_a_number not a json"#,
            Ok(Value::from(btreemap! {})),
        )]);
    }

    #[test]
    fn fails_on_no_match() {
        let rules = parse_grok_rules(
            &[
                "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
                    .to_string(),
            ],
            btreemap! {},
        )
        .expect("couldn't parse rules");
        let error = parse_grok("an ungrokkable message", &rules, false).unwrap_err();

        assert_eq!(error, Error::NoMatch);
    }

    #[test]
    fn appends_to_the_same_field() {
        let rules = parse_grok_rules(
            &[
                r#"%{integer:nested.field} %{notSpace:nested.field:uppercase} %{notSpace:nested.field:nullIf("-")}"#
                    .to_string(),
            ],
            btreemap! {},
        )
            .expect("couldn't parse rules");
        let parsed = parse_grok("1 info -", &rules, false).unwrap();

        assert_eq!(
            parsed,
            Value::from(btreemap! {
                "nested" => btreemap! {
                   "field" =>  Value::Array(vec![1.into(), "INFO".into()]),
                },
            })
        );
    }

    #[test]
    fn error_on_circular_dependency() {
        let err = parse_grok_rules(
            // patterns
            &[r#"%{pattern1}"#.to_string()],
            // aliases with a circular dependency
            btreemap! {
            "pattern1" => r#"%{pattern2}"#.to_string(),
            "pattern2" => r#"%{pattern1}"#.to_string()},
        )
        .unwrap_err();
        assert_eq!(
            format!("{}", err),
            "Circular dependency found in the alias 'pattern1'"
        );
    }

    #[test]
    fn extracts_field_with_regex_capture() {
        test_grok_pattern(vec![(
            r#"(?<field>\w+)"#,
            "abc",
            Ok(Value::Bytes("abc".into())),
        )]);

        // the group name can only be alphanumeric,
        // though we don't validate group names(it would be unnecessary overhead at boot-time),
        // field names are treated as literals, not as lookup paths
        test_full_grok(vec![(
            r#"(?<nested.field.name>\w+)"#,
            "abc",
            Ok(Value::from(btreemap! {
                "nested.field.name" => Value::Bytes("abc".into()),
            })),
        )]);
    }

    #[test]
    fn supports_date_matcher() {
        test_grok_pattern(vec![
            (
                r#"%{date("HH:mm:ss"):field}"#,
                "14:20:15",
                Ok(Value::Integer(51615000)),
            ),
            (
                r#"%{date("dd/MMM/yyyy"):field}"#,
                "06/Mar/2013",
                Ok(Value::Integer(1362528000000)),
            ),
            (
                r#"%{date("EEE MMM dd HH:mm:ss yyyy"):field}"#,
                "Thu Jun 16 08:29:03 2016",
                Ok(Value::Integer(1466065743000)),
            ),
            (
                r#"%{date("dd/MMM/yyyy:HH:mm:ss Z"):field}"#,
                "06/Mar/2013:01:36:30 +0900",
                Ok(Value::Integer(1362501390000)),
            ),
            (
                r#"%{date("yyyy-MM-dd'T'HH:mm:ss.SSSZ"):field}"#,
                "2016-11-29T16:21:36.431+0000",
                Ok(Value::Integer(1480436496431)),
            ),
            (
                r#"%{date("yyyy-MM-dd'T'HH:mm:ss.SSSZZ"):field}"#,
                "2016-11-29T16:21:36.431+00:00",
                Ok(Value::Integer(1480436496431)),
            ),
            (
                r#"%{date("dd/MMM/yyyy:HH:mm:ss.SSS"):field}"#,
                "06/Feb/2009:12:14:14.655",
                Ok(Value::Integer(1233922454655)),
            ),
            (
                r#"%{date("yyyy-MM-dd HH:mm:ss.SSS z"):field}"#,
                "2007-08-31 19:22:22.427 CET",
                Ok(Value::Integer(1188580942427)),
            ),
            (
                r#"%{date("yyyy-MM-dd HH:mm:ss.SSS zzzz"):field}"#,
                "2007-08-31 19:22:22.427 America/Thule",
                Ok(Value::Integer(1188598942427)),
            ),
            (
                r#"%{date("yyyy-MM-dd HH:mm:ss.SSS Z"):field}"#,
                "2007-08-31 19:22:22.427 -03:00",
                Ok(Value::Integer(1188598942427)),
            ),
            (
                r#"%{date("EEE MMM dd HH:mm:ss yyyy", "Europe/Moscow"):field}"#,
                "Thu Jun 16 08:29:03 2016",
                Ok(Value::Integer(1466054943000)),
            ),
            (
                r#"%{date("EEE MMM dd HH:mm:ss yyyy", "UTC+5"):field}"#,
                "Thu Jun 16 08:29:03 2016",
                Ok(Value::Integer(1466047743000)),
            ),
            (
                r#"%{date("EEE MMM dd HH:mm:ss yyyy", "+3"):field}"#,
                "Thu Jun 16 08:29:03 2016",
                Ok(Value::Integer(1466054943000)),
            ),
            (
                r#"%{date("EEE MMM dd HH:mm:ss yyyy", "+03:00"):field}"#,
                "Thu Jun 16 08:29:03 2016",
                Ok(Value::Integer(1466054943000)),
            ),
            (
                r#"%{date("EEE MMM dd HH:mm:ss yyyy", "-0300"):field}"#,
                "Thu Jun 16 08:29:03 2016",
                Ok(Value::Integer(1466076543000)),
            ),
            (
                r#"%{date("MMM d y HH:mm:ss z"):field}"#,
                "Nov 16 2020 13:41:29 GMT",
                Ok(Value::Integer(1605534089000)),
            ),
        ]);

        // check error handling
        assert_eq!(
            parse_grok_rules(&[r#"%{date("ABC:XYZ"):field}"#.to_string()], btreemap! {})
                .unwrap_err()
                .to_string(),
            r#"invalid arguments for the function 'date'"#
        );
        assert_eq!(
            parse_grok_rules(
                &[r#"%{date("EEE MMM dd HH:mm:ss yyyy", "unknown timezone"):field}"#.to_string()],
                btreemap! {},
            )
            .unwrap_err()
            .to_string(),
            r#"invalid arguments for the function 'date'"#
        );
    }

    #[test]
    fn supports_array_filter() {
        test_grok_pattern(vec![
            (
                "%{data:field:array}",
                "[1,2]",
                Ok(Value::Array(vec!["1".into(), "2".into()])),
            ),
            (
                r#"%{data:field:array("\\t")}"#,
                "[1\t2]",
                Ok(Value::Array(vec!["1".into(), "2".into()])),
            ),
            (
                r#"(?m)%{data:field:array("[]","\\n")}"#,
                "[1\n2]",
                Ok(Value::Array(vec!["1".into(), "2".into()])),
            ),
            (
                r#"%{data:field:array("","-")}"#,
                "1-2",
                Ok(Value::Array(vec!["1".into(), "2".into()])),
            ),
            (
                "%{data:field:array(integer)}",
                "[1,2]",
                Ok(Value::Array(vec![1.into(), 2.into()])),
            ),
            (
                r#"%{data:field:array(";", integer)}"#,
                "[1;2]",
                Ok(Value::Array(vec![1.into(), 2.into()])),
            ),
            (
                r#"%{data:field:array("{}",";", integer)}"#,
                "{1;2}",
                Ok(Value::Array(vec![1.into(), 2.into()])),
            ),
            (
                "%{data:field:array(number)}",
                "[1,2]",
                Ok(Value::Array(vec![1.into(), 2.into()])),
            ),
            (
                "%{data:field:array(integer)}",
                "[1,2]",
                Ok(Value::Array(vec![1.into(), 2.into()])),
            ),
            (
                "%{data:field:array(scale(10))}",
                "[1,2.1]",
                Ok(Value::Array(vec![10.into(), 21.into()])),
            ),
            (
                r#"%{data:field:array(";", scale(10))}"#,
                "[1;2.1]",
                Ok(Value::Array(vec![10.into(), 21.into()])),
            ),
            (
                r#"%{data:field:array("{}",";", scale(10))}"#,
                "{1;2.1}",
                Ok(Value::Array(vec![10.into(), 21.into()])),
            ),
        ]);

        test_full_grok(vec![
            // not an array
            (
                r#"%{data:field:array}"#,
                "abc",
                Ok(Value::Object(btreemap! {})),
            ),
            // failed to apply value filter(values are strings)
            (
                r#"%{data:field:array(scale(10))}"#,
                "[a,b]",
                Ok(Value::Object(btreemap! {})),
            ),
        ]);
    }

    #[test]
    fn parses_keyvalue() {
        test_full_grok(vec![
            (
                "%{data::keyvalue}",
                "key=valueStr",
                Ok(Value::from(btreemap! {
                    "key" => "valueStr"
                })),
            ),
            (
                "%{data::keyvalue}",
                "key=<valueStr>",
                Ok(Value::from(btreemap! {
                    "key" => "valueStr"
                })),
            ),
            (
                "%{data::keyvalue}",
                r#""key"="valueStr""#,
                Ok(Value::from(btreemap! {
                    "key" => "valueStr"
                })),
            ),
            (
                "%{data::keyvalue}",
                r#"'key'='valueStr'"#,
                Ok(Value::from(btreemap! {
                   "key" => "valueStr"
                })),
            ),
            (
                "%{data::keyvalue}",
                r#"<key>=<valueStr>"#,
                Ok(Value::from(btreemap! {
                    "key" => "valueStr"
                })),
            ),
            (
                r#"%{data::keyvalue(":")}"#,
                r#"key:valueStr"#,
                Ok(Value::from(btreemap! {
                    "key" => "valueStr"
                })),
            ),
            (
                r#"%{data::keyvalue(":", "/")}"#,
                r#"key:"/valueStr""#,
                Ok(Value::from(btreemap! {
                    "key" => "/valueStr"
                })),
            ),
            (
                r#"%{data::keyvalue(":", "/")}"#,
                r#"/key:/valueStr"#,
                Ok(Value::from(btreemap! {
                    "/key" => "/valueStr"
                })),
            ),
            (
                r#"%{data::keyvalue(":=", "", "{}")}"#,
                r#"key:={valueStr}"#,
                Ok(Value::from(btreemap! {
                    "key" => "valueStr"
                })),
            ),
            (
                r#"%{data::keyvalue("=", "", "", "|")}"#,
                r#"key1=value1|key2=value2"#,
                Ok(Value::from(btreemap! {
                    "key1" => "value1",
                    "key2" => "value2",
                })),
            ),
            (
                r#"%{data::keyvalue("=", "", "", "|")}"#,
                r#"key1="value1"|key2="value2""#,
                Ok(Value::from(btreemap! {
                    "key1" => "value1",
                    "key2" => "value2",
                })),
            ),
            (
                r#"%{data::keyvalue(":=","","<>")}"#,
                r#"key1:=valueStr key2:=</valueStr2> key3:="valueStr3""#,
                Ok(Value::from(btreemap! {
                    "key1" => "valueStr",
                    "key2" => "/valueStr2",
                })),
            ),
            (
                r#"%{data::keyvalue}"#,
                r#"key1=value1,key2=value2"#,
                Ok(Value::from(btreemap! {
                    "key1" => "value1",
                    "key2" => "value2",
                })),
            ),
            (
                r#"%{data::keyvalue}"#,
                r#"key1=value1;key2=value2"#,
                Ok(Value::from(btreemap! {
                    "key1" => "value1",
                    "key2" => "value2",
                })),
            ),
            (
                "%{data::keyvalue}",
                "key:=valueStr",
                Ok(Value::from(btreemap! {})),
            ),
            // empty key or null
            (
                "%{data::keyvalue}",
                "key1= key2=null key3=value3",
                Ok(Value::from(btreemap! {
                    "key3" => "value3"
                })),
            ),
            // empty value or null - comma-separated
            (
                "%{data::keyvalue}",
                "key1=,key2=null,key3= ,key4=value4",
                Ok(Value::from(btreemap! {
                    "key4" => "value4"
                })),
            ),
            // empty key
            (
                "%{data::keyvalue}",
                "=,=value",
                Ok(Value::from(btreemap! {})),
            ),
            // type inference
            (
                "%{data::keyvalue}",
                "float=1.2,boolean=true,null=null,string=abc,integer1=11,integer2=12",
                Ok(Value::from(btreemap! {
                    "float" => Value::Float(NotNan::new(1.2).expect("not a float")),
                    "boolean" => Value::Boolean(true),
                    "string" => Value::Bytes("abc".into()),
                    "integer1" => Value::Integer(11),
                    "integer2" => Value::Integer(12)
                })),
            ),
            // type inference with extra spaces around field delimiters
            (
                "%{data::keyvalue}",
                "float=1.2 , boolean=true , null=null    ,   string=abc , integer1=11  ,  integer2=12  ",
                Ok(Value::from(btreemap! {
                    "float" => Value::Float(NotNan::new(1.2).expect("not a float")),
                    "boolean" => Value::Boolean(true),
                    "string" => Value::Bytes("abc".into()),
                    "integer1" => Value::Integer(11),
                    "integer2" => Value::Integer(12)
                })),
            ),
            // spaces around key-value delimiter are not allowed
            (
                "%{data::keyvalue}",
                "key = valueStr",
                Ok(Value::from(btreemap! {})),
            ),
            (
                "%{data::keyvalue}",
                "key= valueStr",
                Ok(Value::from(btreemap! {})),
            ),
            (
                "%{data::keyvalue}",
                "key =valueStr",
                Ok(Value::from(btreemap! {})),
            ),
            (
                r#"%{data::keyvalue(":")}"#,
                r#"kafka_cluster_status:8ca7b736f0aa43e5"#,
                Ok(Value::from(btreemap! {
                    "kafka_cluster_status" => "8ca7b736f0aa43e5"
                })),
            ),
            (
                r#"%{data::keyvalue}"#,
                r#"field=2.0e"#,
                Ok(Value::from(btreemap! {
                "field" => "2.0e"
                })),
            ),
            (
                r#"%{data::keyvalue("=", "\\w.\\-_@:")}"#,
                r#"IN=eth0 OUT= MAC"#, // no value
                Ok(Value::from(btreemap! {
                    "IN" => "eth0"
                })),
            ),
            (
                "%{data::keyvalue}",
                "db.name=my_db,db.operation=insert",
                Ok(Value::from(btreemap! {
                    "db" => btreemap! {
                        "name" => "my_db",
                        "operation" => "insert",
                    }
                })),
            ),
        ]);
    }

    #[test]
    fn alias_and_main_rule_extract_same_fields_to_array() {
        let rules = parse_grok_rules(
            // patterns
            &[r#"%{notSpace:field:number} %{alias}"#.to_string()],
            // aliases
            btreemap! {
                "alias" => r#"%{notSpace:field:integer}"#.to_string()
            },
        )
        .expect("couldn't parse rules");
        let parsed = parse_grok("1 2", &rules, false).unwrap();

        assert_eq!(
            parsed,
            Value::from(btreemap! {
                 "field" =>  Value::Array(vec![1.into(), 2.into()]),
            })
        );
    }

    #[test]
    fn alias_with_filter() {
        let rules = parse_grok_rules(
            // patterns
            &[r#"%{alias:field:uppercase}"#.to_string()],
            // aliases
            btreemap! {
                "alias" => r#"%{notSpace:subfield1} %{notSpace:subfield2:integer}"#.to_string()
            },
        )
        .expect("couldn't parse rules");
        let parsed = parse_grok("a 1", &rules, false).unwrap();

        assert_eq!(
            parsed,
            Value::from(btreemap! {
                 "field" =>  Value::Bytes("A 1".into()),
                 "subfield1" =>  Value::Bytes("a".into()),
                 "subfield2" =>  Value::Integer(1)
            })
        );
    }

    #[test]
    fn parses_grok_unsafe_field_names() {
        test_full_grok(vec![
            (
                r#"%{data:field["quoted name"]}"#,
                "abc",
                Ok(Value::from(btreemap! {
                "field" => btreemap! {
                    "quoted name" => "abc",
                    }
                })),
            ),
            (
                r#"%{data:@field-name-with-symbols$}"#,
                "abc",
                Ok(Value::from(btreemap! {
                "@field-name-with-symbols$" => "abc"})),
            ),
            (
                r#"%{data:@parent.$child}"#,
                "abc",
                Ok(Value::from(btreemap! {
                "@parent" => btreemap! {
                    "$child" => "abc",
                    }
                })),
            ),
        ]);
    }

    #[test]
    fn parses_with_new_lines() {
        test_full_grok(vec![
            (
                "(?m)%{data:field}",
                "a\nb",
                Ok(Value::from(btreemap! {
                    "field" => "a\nb"
                })),
            ),
            (
                "(?m)%{data:line1}\n%{data:line2}",
                "a\nb",
                Ok(Value::from(btreemap! {
                    "line1" => "a",
                    "line2" => "b"
                })),
            ),
            // no DOTALL mode by default
            ("%{data:field}", "a\nb", Err(Error::NoMatch)),
            // (?s) is not supported by the underlying regex engine(onig) - it uses (?m) instead, so we convert it silently
            (
                "(?s)%{data:field}",
                "a\nb",
                Ok(Value::from(btreemap! {
                    "field" => "a\nb"
                })),
            ),
            // disable DOTALL mode with (?-s)
            ("(?s)(?-s)%{data:field}", "a\nb", Err(Error::NoMatch)),
            // disable and then enable DOTALL mode
            (
                "(?-s)%{data:field} (?s)%{data:field}",
                "abc d\ne",
                Ok(Value::from(btreemap! {
                    "field" => Value::Array(vec!["abc".into(), "d\ne".into()]),
                })),
            ),
        ]);
    }
}
