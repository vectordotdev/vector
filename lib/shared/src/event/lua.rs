use crate::event::{Event, LogEvent, Metric};
use rlua::prelude::*;

use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use std::collections::{BTreeMap, BTreeSet};

pub fn timestamp_to_table(ctx: LuaContext, ts: DateTime<Utc>) -> LuaResult<LuaTable> {
    let table = ctx.create_table()?;
    table.set("year", ts.year())?;
    table.set("month", ts.month())?;
    table.set("day", ts.day())?;
    table.set("hour", ts.hour())?;
    table.set("min", ts.minute())?;
    table.set("sec", ts.second())?;
    table.set("nanosec", ts.nanosecond())?;
    table.set("yday", ts.ordinal())?;
    table.set("wday", ts.weekday().number_from_sunday())?;
    table.set("isdst", false)?;

    Ok(table)
}

pub fn table_is_timestamp(t: &LuaTable<'_>) -> LuaResult<bool> {
    for &key in &["year", "month", "day", "hour", "min", "sec"] {
        if !t.contains_key(key)? {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn table_to_timestamp(t: LuaTable<'_>) -> LuaResult<DateTime<Utc>> {
    let year = t.get("year")?;
    let month = t.get("month")?;
    let day = t.get("day")?;
    let hour = t.get("hour")?;
    let min = t.get("min")?;
    let sec = t.get("sec")?;
    let nano = t.get::<_, Option<u32>>("nanosec")?.unwrap_or(0);
    Ok(Utc.ymd(year, month, day).and_hms_nano(hour, min, sec, nano))
}

pub fn table_to_map<'a, K, V>(t: LuaTable<'a>) -> LuaResult<BTreeMap<K, V>>
where
    K: From<String> + Ord,
    V: FromLua<'a>,
{
    let mut map = BTreeMap::new();
    for pair in t.pairs() {
        let (k, v): (String, V) = pair?;
        map.insert(k.into(), v);
    }
    Ok(map)
}

pub fn table_to_set<'a, T>(t: LuaTable<'a>) -> LuaResult<BTreeSet<T>>
where
    T: FromLua<'a> + Ord,
{
    let mut set = BTreeSet::new();
    for item in t.sequence_values() {
        set.insert(item?);
    }
    Ok(set)
}

pub fn table_is_array(t: &LuaTable) -> LuaResult<bool> {
    Ok(t.len()? > 0)
}

pub fn table_to_array<'a, T>(t: LuaTable<'a>) -> LuaResult<Vec<T>>
where
    T: FromLua<'a>,
{
    let mut seq = Vec::new();
    for item in t.sequence_values() {
        let value = item?;
        seq.push(value);
    }
    Ok(seq)
}

impl<'a> ToLua<'a> for Event {
    fn to_lua(self, ctx: LuaContext<'a>) -> LuaResult<LuaValue> {
        let table = ctx.create_table()?;
        match self {
            Event::Log(log) => table.set("log", log.to_lua(ctx)?)?,
            Event::Metric(metric) => table.set("metric", metric.to_lua(ctx)?)?,
        }
        Ok(LuaValue::Table(table))
    }
}

impl<'a> FromLua<'a> for Event {
    fn from_lua(value: LuaValue<'a>, ctx: LuaContext<'a>) -> LuaResult<Self> {
        let table = match &value {
            LuaValue::Table(t) => t,
            _ => {
                return Err(LuaError::FromLuaConversionError {
                    from: type_name(&value),
                    to: "Event",
                    message: Some("Event should be a Lua table".to_string()),
                })
            }
        };
        match (table.get("log")?, table.get("metric")?) {
            (LuaValue::Table(log), LuaValue::Nil) => {
                Ok(Event::Log(LogEvent::from_lua(LuaValue::Table(log), ctx)?))
            }
            (LuaValue::Nil, LuaValue::Table(metric)) => Ok(Event::Metric(Metric::from_lua(
                LuaValue::Table(metric),
                ctx,
            )?)),
            _ => Err(LuaError::FromLuaConversionError {
                from: type_name(&value),
                to: "Event",
                message: Some(
                    "Event should contain either \"log\" or \"metric\" key at the top level"
                        .to_string(),
                ),
            }),
        }
    }
}

// Taken from https://github.com/amethyst/rlua/blob/v0.17.0/src/value.rs#L52-L61
pub fn type_name(value: &LuaValue) -> &'static str {
    match *value {
        LuaValue::Nil => "nil",
        LuaValue::Boolean(_) => "boolean",
        LuaValue::LightUserData(_) => "light userdata",
        LuaValue::Integer(_) => "integer",
        LuaValue::Number(_) => "number",
        LuaValue::String(_) => "string",
        LuaValue::Table(_) => "table",
        LuaValue::Function(_) => "function",
        LuaValue::Thread(_) => "thread",
        LuaValue::UserData(_) | LuaValue::Error(_) => "userdata",
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{event::*, lookup::*};

    fn assert_event(event: Event, assertions: Vec<&'static str>) {
        Lua::new().context(|ctx| {
            ctx.globals().set("event", event).unwrap();
            for assertion in assertions {
                assert!(
                    ctx.load(assertion).eval::<bool>().expect(assertion),
                    assertion
                );
            }
        });
    }

    #[test_env_log::test]
    fn to_lua_log() {
        let mut event = Event::new_empty_log();
        event.as_mut_log().insert(LookupBuf::from("field"), "value");

        let assertions = vec![
            "type(event) == 'table'",
            "event.metric == nil",
            "type(event.log) == 'table'",
            "event.log.field == 'value'",
        ];

        assert_event(event, assertions);
    }

    #[test_env_log::test]
    fn to_lua_metric() {
        let event = Event::Metric(Metric::new(
            "example counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 0.57721566 },
        ));

        let assertions = vec![
            "type(event) == 'table'",
            "event.log == nil",
            "type(event.metric) == 'table'",
            "event.metric.name == 'example counter'",
            "event.metric.counter.value == 0.57721566",
        ];

        assert_event(event, assertions);
    }

    #[test_env_log::test]
    fn from_lua_log() {
        let lua_event = r#"
        {
            log = {
                field = "example",
                nested = {
                    field = "another example"
                }
            }
        }"#;

        Lua::new().context(|ctx| {
            let event = ctx.load(lua_event).eval::<Event>().unwrap();
            let log = event.as_log();
            assert_eq!(
                log[Lookup::from_str("field").unwrap()],
                Value::Bytes("example".into())
            );
            assert_eq!(
                log[Lookup::from_str("nested.field").unwrap()],
                Value::Bytes("another example".into())
            );
        });
    }

    #[test_env_log::test]
    fn from_lua_metric() {
        let lua_event = r#"
        {
            metric = {
                name = "example counter",
                counter = {
                    value = 0.57721566
                }
            }
        }"#;
        let expected = Event::Metric(Metric::new(
            "example counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 0.57721566 },
        ));

        Lua::new().context(|ctx| {
            let event = ctx.load(lua_event).eval::<Event>().unwrap();
            assert_eq!(event, expected);
        });
    }

    #[test_env_log::test]
    #[should_panic]
    fn from_lua_missing_log_and_metric() {
        let lua_event = r#"{
            some_field: {}
        }"#;
        Lua::new().context(|ctx| ctx.load(lua_event).eval::<Event>().unwrap());
    }
}
