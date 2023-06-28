use mlua::prelude::*;

use crate::event::lua::metric::LuaMetric;
use crate::event::{Event, LogEvent, Metric};

pub struct LuaEvent {
    pub event: Event,
    pub metric_multi_value_tags: bool,
}

impl<'a> ToLua<'a> for LuaEvent {
    #![allow(clippy::wrong_self_convention)] // this trait is defined by mlua
    fn to_lua(self, lua: &'a Lua) -> LuaResult<LuaValue> {
        let table = lua.create_table()?;
        match self.event {
            Event::Log(log) => table.raw_set("log", log.to_lua(lua)?)?,
            Event::Metric(metric) => table.raw_set(
                "metric",
                LuaMetric {
                    metric,
                    multi_value_tags: self.metric_multi_value_tags,
                }
                .to_lua(lua)?,
            )?,
            Event::Trace(_) => {
                return Err(LuaError::ToLuaConversionError {
                    from: "Event",
                    to: "table",
                    message: Some("Trace are not supported".to_string()),
                })
            }
        }
        Ok(LuaValue::Table(table))
    }
}

impl<'a> FromLua<'a> for Event {
    fn from_lua(value: LuaValue<'a>, lua: &'a Lua) -> LuaResult<Self> {
        let LuaValue::Table(table) = &value else {
            return Err(LuaError::FromLuaConversionError {
                from: value.type_name(),
                to: "Event",
                message: Some("Event should be a Lua table".to_string()),
            })
        };
        match (table.raw_get("log")?, table.raw_get("metric")?) {
            (LuaValue::Table(log), LuaValue::Nil) => {
                Ok(Event::Log(LogEvent::from_lua(LuaValue::Table(log), lua)?))
            }
            (LuaValue::Nil, LuaValue::Table(metric)) => Ok(Event::Metric(Metric::from_lua(
                LuaValue::Table(metric),
                lua,
            )?)),
            _ => Err(LuaError::FromLuaConversionError {
                from: value.type_name(),
                to: "Event",
                message: Some(
                    "Event should contain either \"log\" or \"metric\" key at the top level"
                        .to_string(),
                ),
            }),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::event::{
        metric::{MetricKind, MetricValue},
        Metric, Value,
    };

    fn assert_event(event: Event, assertions: Vec<&'static str>) {
        let lua = Lua::new();
        lua.globals()
            .set(
                "event",
                LuaEvent {
                    event,
                    metric_multi_value_tags: false,
                },
            )
            .unwrap();
        for assertion in assertions {
            assert!(
                lua.load(assertion).eval::<bool>().expect(assertion),
                "{}",
                assertion
            );
        }
    }

    #[test]
    fn to_lua_log() {
        let mut event = LogEvent::default();
        event.insert("field", "value");

        let assertions = vec![
            "type(event) == 'table'",
            "event.metric == nil",
            "type(event.log) == 'table'",
            "event.log.field == 'value'",
        ];

        assert_event(event.into(), assertions);
    }

    #[test]
    fn to_lua_metric() {
        let event = Event::Metric(Metric::new(
            "example counter",
            MetricKind::Absolute,
            MetricValue::Counter {
                value: 0.577_215_66,
            },
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

    #[test]
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

        let event = Lua::new().load(lua_event).eval::<Event>().unwrap();
        let log = event.as_log();
        assert_eq!(log["field"], Value::Bytes("example".into()));
        assert_eq!(log["nested.field"], Value::Bytes("another example".into()));
    }

    #[test]
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
            MetricValue::Counter {
                value: 0.577_215_66,
            },
        ));

        let event = Lua::new().load(lua_event).eval::<Event>().unwrap();
        vector_common::assert_event_data_eq!(event, expected);
    }

    #[test]
    #[should_panic]
    fn from_lua_missing_log_and_metric() {
        let lua_event = r#"{
            some_field: {}
        }"#;
        Lua::new().load(lua_event).eval::<Event>().unwrap();
    }
}
