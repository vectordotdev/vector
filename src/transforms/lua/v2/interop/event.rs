use crate::event::{Event, LogEvent, Metric};
use rlua::prelude::*;

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
                    from: value.type_name(),
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

    #[test]
    fn to_lua_log() {
        let mut event = Event::new_empty_log();
        event.as_mut_log().insert("field", "value");

        let assertions = vec![
            "type(event) == 'table'",
            "event.metric == nil",
            "type(event.log) == 'table'",
            "event.log.field == 'value'",
        ];

        assert_event(event, assertions);
    }

    #[test]
    fn to_lua_metric() {
        let event = Event::Metric(Metric::new(
            "example counter".into(),
            None,
            None,
            None,
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

        Lua::new().context(|ctx| {
            let event = ctx.load(lua_event).eval::<Event>().unwrap();
            let log = event.as_log();
            assert_eq!(log["field"], Value::Bytes("example".into()));
            assert_eq!(log["nested.field"], Value::Bytes("another example".into()));
        });
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
            "example counter".into(),
            None,
            None,
            None,
            MetricKind::Absolute,
            MetricValue::Counter { value: 0.57721566 },
        ));

        Lua::new().context(|ctx| {
            let event = ctx.load(lua_event).eval::<Event>().unwrap();
            assert_eq!(event, expected);
        });
    }

    #[test]
    #[should_panic]
    fn from_lua_missing_log_and_metric() {
        let lua_event = r#"{
            some_field: {}
        }"#;
        Lua::new().context(|ctx| ctx.load(lua_event).eval::<Event>().unwrap());
    }
}
