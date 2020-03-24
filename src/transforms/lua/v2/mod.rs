mod interop;

use crate::{
    event::Event,
    topology::config::{DataType, TransformContext},
    transforms::Transform,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Lua error: {}", source))]
    InvalidLua { source: rlua::Error },
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct LuaConfig {
    source: String,
    #[serde(default)]
    search_dirs: Vec<String>,
}

// Implementation of methods from `TransformConfig`
// Note that they are implemented as struct methods instead of trait implementation methods
// because `TransformConfig` trait requires specification of a unique `typetag::serde` name.
// Specifying some name (for example, "lua_v*") results in this name being listed among
// possible configuration options for `transforms` section, but such internal name should not
// be exposed to users.
impl LuaConfig {
    pub fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Lua::new(&self.source, self.search_dirs.clone()).map(|l| {
            let b: Box<dyn Transform> = Box::new(l);
            b
        })
    }

    pub fn input_type(&self) -> DataType {
        DataType::Any
    }

    pub fn output_type(&self) -> DataType {
        DataType::Any
    }

    pub fn transform_type(&self) -> &'static str {
        "lua"
    }
}

// Lua's garbage collector sometimes seems to be not executed automatically on high event rates,
// which leads to leak-like RAM consumption pattern. This constant sets the number of invocations of
// the Lua transform after which GC would be called, thus ensuring that the RAM usage is not too high.
//
// This constant is larger than 1 because calling GC is an expensive operation, so doing it
// after each transform would have significant footprint on the performance.
const GC_INTERVAL: usize = 16;

pub struct Lua {
    lua: rlua::Lua,
    invocations_after_gc: usize,
}

impl Lua {
    pub fn new(source: &str, search_dirs: Vec<String>) -> crate::Result<Self> {
        let lua = rlua::Lua::new();

        let additional_paths = search_dirs
            .into_iter()
            .map(|d| format!("{}/?.lua", d))
            .collect::<Vec<_>>()
            .join(";");

        lua.context(|ctx| {
            if !additional_paths.is_empty() {
                let package = ctx.globals().get::<_, rlua::Table<'_>>("package")?;
                let current_paths = package
                    .get::<_, String>("path")
                    .unwrap_or_else(|_| ";".to_string());
                let paths = format!("{};{}", additional_paths, current_paths);
                package.set("path", paths)?;
            }

            let func = ctx.load(&source).into_function()?;
            ctx.set_named_registry_value("vector_func", func)?;
            Ok(())
        })
        .context(InvalidLua)?;

        Ok(Self {
            lua,
            invocations_after_gc: 0,
        })
    }

    fn process(&mut self, event: Event) -> Result<Option<Event>, rlua::Error> {
        let result = self.lua.context(|ctx| {
            let globals = ctx.globals();

            globals.set("event", event)?;

            let func = ctx.named_registry_value::<_, rlua::Function<'_>>("vector_func")?;
            func.call(())?;
            globals.get::<_, Option<Event>>("event")
        });
        self.invocations_after_gc += 1;
        if self.invocations_after_gc % GC_INTERVAL == 0 {
            self.lua.gc_collect()?;
            self.invocations_after_gc = 0;
        }
        result
    }
}

impl Transform for Lua {
    fn transform(&mut self, event: Event) -> Option<Event> {
        match self.process(event) {
            Ok(event) => event,
            Err(err) => {
                error!(message = "Error in lua script; discarding event.", error = %format_error(&err), rate_limit_secs = 30);
                None
            }
        }
    }
}

fn format_error(error: &rlua::Error) -> String {
    match error {
        rlua::Error::CallbackError { traceback, cause } => format_error(&cause) + "\n" + traceback,
        err => err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{format_error, Lua};
    use crate::{
        event::{
            metric::{Metric, MetricKind, MetricValue},
            Event, Value,
        },
        transforms::Transform,
    };

    #[test]
    fn lua_add_field() {
        let mut transform = Lua::new(
            r#"
              event["log"]["hello"] = "goodbye"
            "#,
            vec![],
        )
        .unwrap();

        let event = Event::from("program me");

        let event = transform.transform(event).unwrap();

        assert_eq!(event.as_log()[&"hello".into()], "goodbye".into());
    }

    #[test]
    fn lua_read_field() {
        let mut transform = Lua::new(
            r#"
              _, _, name = string.find(event.log.message, "Hello, my name is (%a+).")
              event.log.name = name
            "#,
            vec![],
        )
        .unwrap();

        let event = Event::from("Hello, my name is Bob.");

        let event = transform.transform(event).unwrap();

        assert_eq!(event.as_log()[&"name".into()], "Bob".into());
    }

    #[test]
    fn lua_remove_field() {
        let mut transform = Lua::new(
            r#"
              event.log.name = nil
            "#,
            vec![],
        )
        .unwrap();

        let mut event = Event::new_empty_log();
        event.as_mut_log().insert("name", "Bob");
        let event = transform.transform(event).unwrap();

        assert!(event.as_log().get(&"name".into()).is_none());
    }

    #[test]
    fn lua_drop_event() {
        let mut transform = Lua::new(
            r#"
              event = nil
            "#,
            vec![],
        )
        .unwrap();

        let mut event = Event::new_empty_log();
        event.as_mut_log().insert("name", "Bob");
        let event = transform.transform(event);

        assert!(event.is_none());
    }

    #[test]
    fn lua_read_empty_field() {
        let mut transform = Lua::new(
            r#"
              if event["log"]["non-existant"] == nil then
                event["log"]["result"] = "empty"
              else
                event["log"]["result"] = "found"
              end
            "#,
            vec![],
        )
        .unwrap();

        let event = Event::new_empty_log();
        let event = transform.transform(event).unwrap();

        assert_eq!(event.as_log()[&"result".into()], "empty".into());
    }

    #[test]
    fn lua_integer_value() {
        let mut transform = Lua::new(
            r#"
              event["log"]["number"] = 3
            "#,
            vec![],
        )
        .unwrap();

        let event = transform.transform(Event::new_empty_log()).unwrap();
        assert_eq!(event.as_log()[&"number".into()], Value::Integer(3));
    }

    #[test]
    fn lua_numeric_value() {
        let mut transform = Lua::new(
            r#"
              event["log"]["number"] = 3.14159
            "#,
            vec![],
        )
        .unwrap();

        let event = transform.transform(Event::new_empty_log()).unwrap();
        assert_eq!(event.as_log()[&"number".into()], Value::Float(3.14159));
    }

    #[test]
    fn lua_boolean_value() {
        let mut transform = Lua::new(
            r#"
              event["log"]["bool"] = true
            "#,
            vec![],
        )
        .unwrap();

        let event = transform.transform(Event::new_empty_log()).unwrap();
        assert_eq!(event.as_log()[&"bool".into()], Value::Boolean(true));
    }

    #[test]
    fn lua_non_coercible_value() {
        let mut transform = Lua::new(
            r#"
              event["log"]["junk"] = nil
            "#,
            vec![],
        )
        .unwrap();

        let event = transform.transform(Event::new_empty_log()).unwrap();
        assert_eq!(event.as_log().get(&"junk".into()), None);
    }

    #[test]
    fn lua_non_string_key_write() {
        let mut transform = Lua::new(
            r#"
              event["log"][false] = "hello"
            "#,
            vec![],
        )
        .unwrap();

        let err = transform.process(Event::new_empty_log()).unwrap_err();
        let err = format_error(&err);
        assert!(err.contains("error converting Lua boolean to String"), err);
    }

    #[test]
    fn lua_non_string_key_read() {
        let mut transform = Lua::new(
            r#"
            event.log.result = event.log[false]
            "#,
            vec![],
        )
        .unwrap();

        let event = transform.transform(Event::new_empty_log()).unwrap();
        assert_eq!(event.as_log().get(&"result".into()), None);
    }

    #[test]
    fn lua_script_error() {
        let mut transform = Lua::new(
            r#"
              error("this is an error")
            "#,
            vec![],
        )
        .unwrap();

        let err = transform.process(Event::new_empty_log()).unwrap_err();
        let err = format_error(&err);
        assert!(err.contains("this is an error"), err);
    }

    #[test]
    fn lua_syntax_error() {
        let err = Lua::new(
            r#"
              1234 = sadf <>&*!#@
            "#,
            vec![],
        )
        .map(|_| ())
        .unwrap_err()
        .to_string();

        assert!(err.contains("syntax error:"), err);
    }

    #[test]
    fn lua_load_file() {
        use std::fs::File;
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();

        let mut file = File::create(dir.path().join("script2.lua")).unwrap();
        write!(
            &mut file,
            r#"
              local M = {{}}

              local function modify(event2)
                event2["log"]["new field"] = "new value"
              end
              M.modify = modify

              return M
            "#
        )
        .unwrap();

        let source = r#"
          local script2 = require("script2")
          script2.modify(event)
        "#;

        let mut transform =
            Lua::new(source, vec![dir.path().to_string_lossy().into_owned()]).unwrap();
        let event = Event::new_empty_log();
        let event = transform.transform(event).unwrap();

        assert_eq!(event.as_log()[&"new field".into()], "new value".into());
    }

    #[test]
    fn lua_pairs() {
        let mut transform = Lua::new(
            r#"
              for k,v in pairs(event.log) do
                event.log[k] = k .. v
              end
            "#,
            vec![],
        )
        .unwrap();

        let mut event = Event::new_empty_log();
        event.as_mut_log().insert("name", "Bob");
        event.as_mut_log().insert("friend", "Alice");

        let event = transform.transform(event).unwrap();

        assert_eq!(event.as_log()[&"name".into()], "nameBob".into());
        assert_eq!(event.as_log()[&"friend".into()], "friendAlice".into());
    }

    #[test]
    fn lua_metric() {
        let mut transform = Lua::new(
            r#"
            event.metric.counter.value = event.metric.counter.value + 1
            "#,
            vec![],
        )
        .unwrap();

        let event = Event::Metric(Metric {
            name: "example counter".into(),
            timestamp: None,
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::Counter { value: 1.0 },
        });

        let expected = Event::Metric(Metric {
            name: "example counter".into(),
            timestamp: None,
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::Counter { value: 2.0 },
        });

        let event = transform.transform(event).unwrap();

        assert_eq!(event, expected);
    }
}
