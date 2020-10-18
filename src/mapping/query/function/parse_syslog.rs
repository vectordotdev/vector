use super::prelude::*;
use chrono::DateTime;
use chrono::Datelike;
use chrono::Utc;
use std::collections::BTreeMap;
use syslog_loose::{IncompleteDate, Message, ProcId};

#[derive(Debug)]
pub(in crate::mapping) struct ParseSyslogFn {
    query: Box<dyn Function>,
}

impl ParseSyslogFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

/// Function used to resolve the year for syslog messages that don't include the year.
/// If the current month is January, and the syslog message is for December, it will take the previous year.
/// Otherwise, take the current year.
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

    result.insert("message".to_string(), Value::from(message.msg.to_string()));

    if let Some(host) = message.hostname {
        result.insert("hostname".to_string(), Value::from(host.to_string()));
    }

    if let Some(severity) = message.severity {
        result.insert(
            "severity".to_string(),
            Value::from(severity.as_str().to_owned()),
        );
    }

    if let Some(facility) = message.facility {
        result.insert(
            "facility".to_string(),
            Value::from(facility.as_str().to_owned()),
        );
    }

    if let Some(app_name) = message.appname {
        result.insert("appname".to_string(), Value::from(app_name.to_owned()));
    }

    if let Some(msg_id) = message.msgid {
        result.insert("msgid".to_string(), Value::from(msg_id.to_owned()));
    }

    if let Some(timestamp) = message.timestamp {
        let timestamp: DateTime<Utc> = timestamp.into();
        result.insert("timestamp".to_string(), Value::from(timestamp));
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
            result.insert(key, Value::from(value.to_string()));
        }
    }

    Value::from(result)
}

impl Function for ParseSyslogFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let message =
            required!(ctx, self.query, Value::Bytes(v) => String::from_utf8_lossy(&v).into_owned());

        let parsed = syslog_loose::parse_message_with_year(&message, resolve_year);

        Ok(message_to_value(parsed))
    }

    fn parameters() -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }
}

impl TryFrom<ArgumentList> for ParseSyslogFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;

        Ok(Self { query })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;
    use chrono::prelude::*;

    #[test]
    fn parses() {
        let cases = vec![(
            Event::from(""),
            Ok(Value::from({
                let mut map = BTreeMap::new();
                map.insert("severity".to_string(), Value::from("notice"));
                map.insert("facility".to_string(), Value::from("user"));
                map.insert(
                    "timestamp".to_string(),
                    Value::from(chrono::Utc.ymd(2020, 3, 13).and_hms_milli(20, 45, 38, 119)),
                );
                map.insert("hostname".to_string(), Value::from("dynamicwireless.name"));
                map.insert("appname".to_string(), Value::from("non"));
                map.insert("procid".to_string(), Value::from(2426));
                map.insert("msgid".to_string(), Value::from("ID931"));
                map.insert("exampleSDID@32473.iut".to_string(), Value::from("3"));
                map.insert(
                    "exampleSDID@32473.eventSource".to_string(),
                    Value::from("Application"),
                );
                map.insert("exampleSDID@32473.eventID".to_string(), Value::from("1011"));
                map.insert(
                    "message".to_string(),
                    Value::from(
                        "Try to override the THX port, maybe it will reboot the neural interface!",
                    ),
                );
                map
            })),
            ParseSyslogFn::new(Box::new(Literal::from(Value::from(
                r#"<13>1 2020-03-13T20:45:38.119Z dynamicwireless.name non 2426 ID931 [exampleSDID@32473 iut="3" eventSource= "Application" eventID="1011"] Try to override the THX port, maybe it will reboot the neural interface!"#,
            )))),
        )];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    #[should_panic(expected = "unexpected value type: 'integer'")]
    fn invalid_type() {
        let mut event = Event::from("");
        event.as_mut_log().insert("foo", Value::Integer(42));

        let _ = ParseSyslogFn::new(Box::new(Path::from(vec![vec!["foo"]]))).execute(&event);
    }
}
