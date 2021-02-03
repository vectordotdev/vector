use chrono::{DateTime, Datelike, Utc};
use remap::prelude::*;
use remap::value::Kind;
use std::collections::BTreeMap;
use syslog_loose::{IncompleteDate, Message, ProcId};

#[derive(Clone, Copy, Debug)]
pub struct ParseSyslog;

impl Function for ParseSyslog {
    fn identifier(&self) -> &'static str {
        "parse_syslog"
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

        Ok(Box::new(ParseSyslogFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ParseSyslogFn {
    value: Box<dyn Expression>,
}

impl ParseSyslogFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

/// Function used to resolve the year for syslog messages that don't include the
/// year. If the current month is January, and the syslog message is for
/// December, it will take the previous year. Otherwise, take the current year.
fn resolve_year((month, _date, _hour, _min, _sec): IncompleteDate) -> i32 {
    let now = Utc::now();
    if now.month() == 1 && month == 12 {
        now.year() - 1
    } else {
        now.year()
    }
}

/// Create a Value::Map from the fields of the given syslog message.
fn message_to_value(message: Message<&str>) -> Value {
    let mut result = BTreeMap::new();

    result.insert("message".to_string(), message.msg.to_string().into());

    if let Some(host) = message.hostname {
        result.insert("hostname".to_string(), host.to_string().into());
    }

    if let Some(severity) = message.severity {
        result.insert("severity".to_string(), severity.as_str().to_owned().into());
    }

    if let Some(facility) = message.facility {
        result.insert("facility".to_string(), facility.as_str().to_owned().into());
    }

    if let Some(app_name) = message.appname {
        result.insert("appname".to_string(), app_name.to_owned().into());
    }

    if let Some(msg_id) = message.msgid {
        result.insert("msgid".to_string(), msg_id.to_owned().into());
    }

    if let Some(timestamp) = message.timestamp {
        let timestamp: DateTime<Utc> = timestamp.into();
        result.insert("timestamp".to_string(), timestamp.into());
    }

    if let Some(procid) = message.procid {
        let value: Value = match procid {
            ProcId::PID(pid) => pid.into(),
            ProcId::Name(name) => name.to_string().into(),
        };
        result.insert("procid".to_string(), value);
    }

    for element in message.structured_data.into_iter() {
        for (name, value) in element.params.into_iter() {
            let key = format!("{}.{}", element.id, name);
            result.insert(key, value.to_string().into());
        }
    }

    result.into()
}

impl Expression for ParseSyslogFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;
        let message = String::from_utf8_lossy(&bytes);

        let parsed = syslog_loose::parse_message_with_year_exact(&message, resolve_year)?;

        Ok(message_to_value(parsed))
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .into_fallible(true)
            .with_constraint(Kind::Map)
            .with_inner_type(inner_type_def())
    }
}

fn inner_type_def() -> Option<InnerTypeDef> {
    Some(inner_type_def! ({
        "message": Kind::Bytes,
        "hostname": Kind::Bytes | Kind::Null,
        "severity": Kind::Bytes | Kind::Null,
        "facility": Kind::Bytes | Kind::Null,
        "appname": Kind::Bytes | Kind::Null,
        "msgid": Kind::Bytes | Kind::Null,
        "timestamp": Kind::Timestamp | Kind::Null,
        "procid": Kind::Bytes | Kind::Integer | Kind::Null
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::prelude::*;
    use shared::btreemap;

    remap::test_type_def![
        value_string {
            expr: |_| ParseSyslogFn { value: Literal::from("foo").boxed() },
            def: TypeDef { kind: Kind::Map,
                           fallible: true,
                           inner_type_def: inner_type_def(),
            },
        }

        value_non_string {
            expr: |_| ParseSyslogFn { value: Literal::from(1).boxed() },
            def: TypeDef { fallible: true,
                           kind: Kind::Map,
                           inner_type_def: inner_type_def(),
            },
        }

        value_optional {
            expr: |_| ParseSyslogFn { value: Box::new(Noop) },
            def: TypeDef { fallible: true,
                           kind: Kind::Map,
                           inner_type_def: inner_type_def(),
            },
        }
    ];

    remap::test_function![
        parse_syslog => ParseSyslog;

        valid {
            args: func_args![value: r#"<13>1 2020-03-13T20:45:38.119Z dynamicwireless.name non 2426 ID931 [exampleSDID@32473 iut="3" eventSource= "Application" eventID="1011"] Try to override the THX port, maybe it will reboot the neural interface!"#],
            want: Ok(btreemap! {
                "severity" => "notice",
                "facility" => "user",
                "timestamp" => chrono::Utc.ymd(2020, 3, 13).and_hms_milli(20, 45, 38, 119),
                "hostname" => "dynamicwireless.name",
                "appname" => "non",
                "procid" => 2426,
                "msgid" => "ID931",
                "exampleSDID@32473.iut" => "3",
                "exampleSDID@32473.eventSource" => "Application",
                "exampleSDID@32473.eventID" => "1011",
                "message" => "Try to override the THX port, maybe it will reboot the neural interface!",
            })
        }

        invalid {
            args: func_args![value: "not much of a syslog message"],
            want: Err("function call error: unable to parse input as valid syslog message".to_string())
        }

        haproxy {
            args: func_args![value: r#"<133>Jun 13 16:33:35 haproxy[73411]: Proxy sticky-servers started."#],
            want: Ok(btreemap! {
                    "facility" => "local0",
                    "severity" => "notice",
                    "message" => "Proxy sticky-servers started.",
                    "timestamp" => DateTime::<Utc>::from(chrono::Local.ymd(Utc::now().year(), 6, 13).and_hms_milli(16, 33, 35, 0)),
                    "appname" => "haproxy",
                    "procid" => 73411,
            })
        }

        missing_pri {
            args: func_args![value: r#"Jun 13 16:33:35 haproxy[73411]: I am missing a pri."#],
            want: Ok(btreemap! {
                "message" => "I am missing a pri.",
                "timestamp" => DateTime::<Utc>::from(chrono::Local.ymd(Utc::now().year(), 6, 13).and_hms_milli(16, 33, 35, 0)),
                "appname" => "haproxy",
                "procid" => 73411,
            })
        }
    ];

    #[test]
    fn handles_empty_sd_element() {
        fn there_is_map_called_empty(value: Value) -> Result<bool> {
            match value {
                Value::Map(map) => {
                    Ok(map.iter().find(|(key, _)| (&key[..]).starts_with("empty")) == None)
                }
                _ => Err("Result was not a map".into()),
            }
        }

        let mut state = state::Program::default();
        let mut object: Value = btreemap! {}.into();

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[empty]"#
        );

        let query = ParseSyslogFn::new(Box::new(Literal::from(msg)));
        let value = query.execute(&mut state, &mut object).unwrap();
        assert!(there_is_map_called_empty(value).unwrap());

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[non_empty x="1"][empty]"#
        );

        let query = ParseSyslogFn::new(Box::new(Literal::from(msg)));
        let value = query.execute(&mut state, &mut object).unwrap();
        assert!(there_is_map_called_empty(value).unwrap());

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[empty][non_empty x="1"]"#
        );

        let query = ParseSyslogFn::new(Box::new(Literal::from(msg)));
        let value = query.execute(&mut state, &mut object).unwrap();
        assert!(there_is_map_called_empty(value).unwrap());

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[empty not_really="testing the test"]"#
        );

        let query = ParseSyslogFn::new(Box::new(Literal::from(msg)));
        let value = query.execute(&mut state, &mut object).unwrap();
        assert!(!there_is_map_called_empty(value).unwrap());
    }
}
