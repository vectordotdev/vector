use crate::{event::*, lookup::*};
use rlua::prelude::*;

impl<'a> ToLua<'a> for LogEvent {
    fn to_lua(self, ctx: LuaContext<'a>) -> LuaResult<LuaValue> {
        ctx.create_table_from(self.into_iter().map(|(k, v)| (k, v)))
            .map(LuaValue::Table)
    }
}

impl<'a> FromLua<'a> for LogEvent {
    fn from_lua(value: LuaValue<'a>, _: LuaContext<'a>) -> LuaResult<Self> {
        match value {
            LuaValue::Table(t) => {
                let mut log = LogEvent::default();
                for pair in t.pairs() {
                    let (key, value): (String, Value) = pair?;
                    let key = LookupBuf::from(key);
                    log.insert(key, value);
                }
                Ok(log)
            }
            _ => Err(rlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LogEvent",
                message: Some("LogEvent should ba a Lua table".to_string()),
            }),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rlua::Lua;

    #[test_env_log::test]
    fn to_lua() {
        let mut event = Event::new_empty_log();
        let log = event.as_mut_log();
        log.insert("a", 1);
        log.insert(LookupBuf::from_str("nested.field").unwrap(), "2");
        log.insert(
            LookupBuf::from_str("nested.array[0]").unwrap(),
            "example value",
        );
        log.insert(
            LookupBuf::from_str("nested.array[2]").unwrap(),
            "another value",
        );

        let assertions = vec![
            "type(log) == 'table'",
            "log.a == 1",
            "type(log.nested) == 'table'",
            "log.nested.field == '2'",
            "#log.nested.array == 3",
            "log.nested.array[1] == 'example value'",
            "log.nested.array[2] == ''",
            "log.nested.array[3] == 'another value'",
        ];

        Lua::new().context(move |ctx| {
            ctx.globals().set("log", log.clone()).unwrap();
            for assertion in assertions {
                let result: bool = ctx
                    .load(assertion)
                    .eval()
                    .unwrap_or_else(|_| panic!("Failed to verify assertion {:?}", assertion));
                assert!(result, assertion);
            }
        });
    }

    #[test_env_log::test]
    fn from_lua() {
        let lua_event = r#"
        {
            a = 1,
            nested = {
                field = '2',
                array = {'example value', '', 'another value'}
            }
        }
        "#;
        Lua::new().context(move |ctx| {
            let event: LogEvent = ctx.load(lua_event).eval().unwrap();

            assert_eq!(event["a"], Value::Integer(1));
            assert_eq!(
                event[Lookup::from_str("nested.field").unwrap()],
                Value::Bytes("2".into())
            );
            assert_eq!(
                event[Lookup::from_str("nested.array[0]").unwrap()],
                Value::Bytes("example value".into())
            );
            assert_eq!(
                event[Lookup::from_str("nested.array[1]").unwrap()],
                Value::Bytes("".into())
            );
            assert_eq!(
                event[Lookup::from_str("nested.array[2]").unwrap()],
                Value::Bytes("another value".into())
            );
        });
    }
}
