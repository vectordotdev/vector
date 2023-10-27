use std::{path::PathBuf, sync::Arc, time::Duration};

use serde_with::serde_as;
use snafu::{ResultExt, Snafu};
use vector_lib::codecs::MetricTagValues;
use vector_lib::configurable::configurable_component;
pub use vector_lib::event::lua;
use vector_lib::transform::runtime_transform::{RuntimeTransform, Timer};

use crate::config::{ComponentKey, OutputId};
use crate::event::lua::event::LuaEvent;
use crate::schema::Definition;
use crate::{
    config::{self, DataType, Input, TransformOutput, CONFIG_PATHS},
    event::Event,
    internal_events::{LuaBuildError, LuaGcTriggered},
    schema,
    transforms::Transform,
};

#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("Invalid \"search_dirs\": {}", source))]
    InvalidSearchDirs { source: mlua::Error },
    #[snafu(display("Cannot evaluate Lua code in \"source\": {}", source))]
    InvalidSource { source: mlua::Error },

    #[snafu(display("Cannot evaluate Lua code defining \"hooks.init\": {}", source))]
    InvalidHooksInit { source: mlua::Error },
    #[snafu(display("Cannot evaluate Lua code defining \"hooks.process\": {}", source))]
    InvalidHooksProcess { source: mlua::Error },
    #[snafu(display("Cannot evaluate Lua code defining \"hooks.shutdown\": {}", source))]
    InvalidHooksShutdown { source: mlua::Error },
    #[snafu(display("Cannot evaluate Lua code defining timer handler: {}", source))]
    InvalidTimerHandler { source: mlua::Error },

    #[snafu(display("Runtime error in \"hooks.init\" function: {}", source))]
    RuntimeErrorHooksInit { source: mlua::Error },
    #[snafu(display("Runtime error in \"hooks.process\" function: {}", source))]
    RuntimeErrorHooksProcess { source: mlua::Error },
    #[snafu(display("Runtime error in \"hooks.shutdown\" function: {}", source))]
    RuntimeErrorHooksShutdown { source: mlua::Error },
    #[snafu(display("Runtime error in timer handler: {}", source))]
    RuntimeErrorTimerHandler { source: mlua::Error },

    #[snafu(display("Cannot call GC in Lua runtime: {}", source))]
    RuntimeErrorGc { source: mlua::Error },
}

/// Configuration for the version two of the `lua` transform.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct LuaConfig {
    /// The Lua program to initialize the transform with.
    ///
    /// The program can be used to to import external dependencies, as well as define the functions
    /// used for the various lifecycle hooks. However, it's not strictly required, as the lifecycle
    /// hooks can be configured directly with inline Lua source for each respective hook.
    #[configurable(metadata(
        docs::examples = "function init()\n\tcount = 0\nend\n\nfunction process()\n\tcount = count + 1\nend\n\nfunction timer_handler(emit)\n\temit(make_counter(counter))\n\tcounter = 0\nend\n\nfunction shutdown(emit)\n\temit(make_counter(counter))\nend\n\nfunction make_counter(value)\n\treturn metric = {\n\t\tname = \"event_counter\",\n\t\tkind = \"incremental\",\n\t\ttimestamp = os.date(\"!*t\"),\n\t\tcounter = {\n\t\t\tvalue = value\n\t\t}\n \t}\nend",
        docs::examples = "-- external file with hooks and timers defined\nrequire('custom_module')",
    ))]
    source: Option<String>,

    /// A list of directories to search when loading a Lua file via the `require` function.
    ///
    /// If not specified, the modules are looked up in the configuration directories.
    #[serde(default = "default_config_paths")]
    #[configurable(metadata(docs::examples = "/etc/vector/lua"))]
    #[configurable(metadata(docs::human_name = "Search Directories"))]
    search_dirs: Vec<PathBuf>,

    #[configurable(derived)]
    hooks: HooksConfig,

    /// A list of timers which should be configured and executed periodically.
    #[serde(default)]
    timers: Vec<TimerConfig>,

    /// When set to `single`, metric tag values are exposed as single strings, the
    /// same as they were before this config option. Tags with multiple values show the last assigned value, and null values
    /// are ignored.
    ///
    /// When set to `full`, all metric tags are exposed as arrays of either string or null
    /// values.
    #[serde(default)]
    metric_tag_values: MetricTagValues,
}

fn default_config_paths() -> Vec<PathBuf> {
    match CONFIG_PATHS.lock().ok() {
        Some(config_paths) => config_paths
            .clone()
            .into_iter()
            .map(|config_path| match config_path {
                config::ConfigPath::File(mut path, _format) => {
                    path.pop();
                    path
                }
                config::ConfigPath::Dir(path) => path,
            })
            .collect(),
        None => vec![],
    }
}

/// Lifecycle hooks.
///
/// These hooks can be set to perform additional processing during the lifecycle of the transform.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
struct HooksConfig {
    /// The function called when the first event comes in, before `hooks.process` is called.
    ///
    /// It can produce new events using the `emit` function.
    ///
    /// This can either be inline Lua that defines a closure to use, or the name of the Lua function to call. In both
    /// cases, the closure/function takes a single parameter, `emit`, which is a reference to a function for emitting events.
    #[configurable(metadata(
        docs::examples = "function (emit)\n\t-- Custom Lua code here\nend",
        docs::examples = "init",
    ))]
    init: Option<String>,

    /// The function called for each incoming event.
    ///
    /// It can produce new events using the `emit` function.
    ///
    /// This can either be inline Lua that defines a closure to use, or the name of the Lua function to call. In both
    /// cases, the closure/function takes two parameters. The first parameter, `event`, is the event being processed,
    /// while the second parameter, `emit`, is a reference to a function for emitting events.
    #[configurable(metadata(
        docs::examples = "function (event, emit)\n\tevent.log.field = \"value\" -- set value of a field\n\tevent.log.another_field = nil -- remove field\n\tevent.log.first, event.log.second = nil, event.log.first -- rename field\n\t-- Very important! Emit the processed event.\n\temit(event)\nend",
        docs::examples = "process",
    ))]
    process: String,

    /// The function called when the transform is stopped.
    ///
    /// It can produce new events using the `emit` function.
    ///
    /// This can either be inline Lua that defines a closure to use, or the name of the Lua function to call. In both
    /// cases, the closure/function takes a single parameter, `emit`, which is a reference to a function for emitting events.
    #[configurable(metadata(
        docs::examples = "function (emit)\n\t-- Custom Lua code here\nend",
        docs::examples = "shutdown",
    ))]
    shutdown: Option<String>,
}

/// A Lua timer.
#[serde_as]
#[configurable_component]
#[derive(Clone, Debug)]
struct TimerConfig {
    /// The interval to execute the handler, in seconds.
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::human_name = "Interval"))]
    interval_seconds: Duration,

    /// The handler function which is called when the timer ticks.
    ///
    /// It can produce new events using the `emit` function.
    ///
    /// This can either be inline Lua that defines a closure to use, or the name of the Lua function
    /// to call. In both cases, the closure/function takes a single parameter, `emit`, which is a
    /// reference to a function for emitting events.
    #[configurable(metadata(docs::examples = "timer_handler"))]
    handler: String,
}

impl LuaConfig {
    pub fn build(&self, key: ComponentKey) -> crate::Result<Transform> {
        Lua::new(self, key).map(Transform::event_task)
    }

    pub fn input(&self) -> Input {
        Input::new(DataType::Metric | DataType::Log)
    }

    pub fn outputs(
        &self,
        input_definitions: &[(OutputId, schema::Definition)],
    ) -> Vec<TransformOutput> {
        // Lua causes the type definition to be reset
        let namespaces = input_definitions
            .iter()
            .flat_map(|(_output, definition)| definition.log_namespaces().clone())
            .collect();

        let definition = input_definitions
            .iter()
            .map(|(output, _definition)| {
                (
                    output.clone(),
                    Definition::default_for_namespace(&namespaces),
                )
            })
            .collect();

        vec![TransformOutput::new(
            DataType::Metric | DataType::Log,
            definition,
        )]
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
    lua: mlua::Lua,
    invocations_after_gc: usize,
    hook_init: Option<mlua::RegistryKey>,
    hook_process: mlua::RegistryKey,
    hook_shutdown: Option<mlua::RegistryKey>,
    timers: Vec<(Timer, mlua::RegistryKey)>,
    multi_value_tags: bool,
    source_id: Arc<ComponentKey>,
}

// Helper to create `RegistryKey` from Lua function code
fn make_registry_value(lua: &mlua::Lua, source: &str) -> mlua::Result<mlua::RegistryKey> {
    lua.load(source)
        .eval::<mlua::Function>()
        .and_then(|f| lua.create_registry_value(f))
}

impl Lua {
    pub fn new(config: &LuaConfig, key: ComponentKey) -> crate::Result<Self> {
        // In order to support loading C modules in Lua, we need to create unsafe instance
        // without debug library.
        let lua = unsafe {
            mlua::Lua::unsafe_new_with(mlua::StdLib::ALL_SAFE, mlua::LuaOptions::default())
        };

        let additional_paths = config
            .search_dirs
            .iter()
            .map(|d| format!("{}/?.lua", d.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(";");

        let mut timers = Vec::new();

        if !additional_paths.is_empty() {
            let package = lua.globals().get::<_, mlua::Table<'_>>("package")?;
            let current_paths = package
                .get::<_, String>("path")
                .unwrap_or_else(|_| ";".to_string());
            let paths = format!("{};{}", additional_paths, current_paths);
            package.set("path", paths)?;
        }

        if let Some(source) = &config.source {
            lua.load(source).eval().context(InvalidSourceSnafu)?;
        }

        let hook_init_code = config.hooks.init.as_ref();
        let hook_init = hook_init_code
            .map(|code| make_registry_value(&lua, code))
            .transpose()
            .context(InvalidHooksInitSnafu)?;

        let hook_process =
            make_registry_value(&lua, &config.hooks.process).context(InvalidHooksProcessSnafu)?;

        let hook_shutdown_code = config.hooks.shutdown.as_ref();
        let hook_shutdown = hook_shutdown_code
            .map(|code| make_registry_value(&lua, code))
            .transpose()
            .context(InvalidHooksShutdownSnafu)?;

        for (id, timer) in config.timers.iter().enumerate() {
            let handler_key = lua
                .load(&timer.handler)
                .eval::<mlua::Function>()
                .and_then(|f| lua.create_registry_value(f))
                .context(InvalidTimerHandlerSnafu)?;

            let timer = Timer {
                id: id as u32,
                interval: timer.interval_seconds,
            };
            timers.push((timer, handler_key));
        }

        let multi_value_tags = config.metric_tag_values == MetricTagValues::Full;

        Ok(Self {
            lua,
            invocations_after_gc: 0,
            timers,
            hook_init,
            hook_process,
            hook_shutdown,
            multi_value_tags,
            source_id: Arc::new(key),
        })
    }

    #[cfg(test)]
    fn process(&mut self, event: Event, output: &mut Vec<Event>) -> Result<(), mlua::Error> {
        let source_id = event.source_id().cloned();
        let lua = &self.lua;
        let result = lua.scope(|scope| {
            let emit = scope.create_function_mut(|_, mut event: Event| {
                if let Some(source_id) = &source_id {
                    event.set_source_id(Arc::clone(source_id));
                }
                output.push(event);
                Ok(())
            })?;

            lua.registry_value::<mlua::Function>(&self.hook_process)?
                .call((
                    LuaEvent {
                        event,
                        metric_multi_value_tags: self.multi_value_tags,
                    },
                    emit,
                ))
        });

        self.attempt_gc();
        result
    }

    #[cfg(test)]
    fn process_single(&mut self, event: Event) -> Result<Option<Event>, mlua::Error> {
        let mut out = Vec::new();
        self.process(event, &mut out)?;
        assert!(out.len() <= 1);
        Ok(out.into_iter().next())
    }

    fn attempt_gc(&mut self) {
        self.invocations_after_gc += 1;
        if self.invocations_after_gc % GC_INTERVAL == 0 {
            emit!(LuaGcTriggered {
                used_memory: self.lua.used_memory()
            });
            _ = self
                .lua
                .gc_collect()
                .context(RuntimeErrorGcSnafu)
                .map_err(|error| error!(%error, rate_limit = 30));
            self.invocations_after_gc = 0;
        }
    }
}

// A helper that reduces code duplication.
fn wrap_emit_fn<'lua, 'scope, F: 'scope>(
    scope: &mlua::Scope<'lua, 'scope>,
    mut emit_fn: F,
    source_id: Arc<ComponentKey>,
) -> mlua::Result<mlua::Function<'lua>>
where
    F: FnMut(Event),
{
    scope.create_function_mut(move |_, mut event: Event| -> mlua::Result<()> {
        event.set_source_id(Arc::clone(&source_id));
        emit_fn(event);
        Ok(())
    })
}

impl RuntimeTransform for Lua {
    fn hook_process<F>(&mut self, event: Event, emit_fn: F)
    where
        F: FnMut(Event),
    {
        let lua = &self.lua;
        let source_id = Arc::clone(event.source_id().unwrap_or(&self.source_id));
        _ = lua
            .scope(|scope| -> mlua::Result<()> {
                lua.registry_value::<mlua::Function>(&self.hook_process)?
                    .call((
                        LuaEvent {
                            event,
                            metric_multi_value_tags: self.multi_value_tags,
                        },
                        wrap_emit_fn(scope, emit_fn, source_id)?,
                    ))
            })
            .context(RuntimeErrorHooksProcessSnafu)
            .map_err(|e| emit!(LuaBuildError { error: e }));

        self.attempt_gc();
    }

    fn hook_init<F>(&mut self, emit_fn: F)
    where
        F: FnMut(Event),
    {
        let lua = &self.lua;
        _ = lua
            .scope(|scope| -> mlua::Result<()> {
                match &self.hook_init {
                    Some(key) => lua
                        .registry_value::<mlua::Function>(key)?
                        .call(wrap_emit_fn(scope, emit_fn, Arc::clone(&self.source_id))?),
                    None => Ok(()),
                }
            })
            .context(RuntimeErrorHooksInitSnafu)
            .map_err(|error| error!(%error, rate_limit = 30));

        self.attempt_gc();
    }

    fn hook_shutdown<F>(&mut self, emit_fn: F)
    where
        F: FnMut(Event),
    {
        let lua = &self.lua;
        _ = lua
            .scope(|scope| -> mlua::Result<()> {
                match &self.hook_shutdown {
                    Some(key) => lua
                        .registry_value::<mlua::Function>(key)?
                        .call(wrap_emit_fn(scope, emit_fn, Arc::clone(&self.source_id))?),
                    None => Ok(()),
                }
            })
            .context(RuntimeErrorHooksShutdownSnafu)
            .map_err(|error| error!(%error, rate_limit = 30));

        self.attempt_gc();
    }

    fn timer_handler<F>(&mut self, timer: Timer, emit_fn: F)
    where
        F: FnMut(Event),
    {
        let lua = &self.lua;
        _ = lua
            .scope(|scope| -> mlua::Result<()> {
                let handler_key = &self.timers[timer.id as usize].1;
                lua.registry_value::<mlua::Function>(handler_key)?
                    .call(wrap_emit_fn(scope, emit_fn, Arc::clone(&self.source_id))?)
            })
            .context(RuntimeErrorTimerHandlerSnafu)
            .map_err(|error| error!(%error, rate_limit = 30));

        self.attempt_gc();
    }

    fn timers(&self) -> Vec<Timer> {
        self.timers.iter().map(|(timer, _)| *timer).collect()
    }
}

#[cfg(test)]
mod tests {
    use std::{future::Future, sync::Arc};

    use similar_asserts::assert_eq;
    use tokio::sync::mpsc::{self, Receiver, Sender};
    use tokio::sync::Mutex;
    use tokio_stream::wrappers::ReceiverStream;

    use super::*;
    use crate::test_util::{components::assert_transform_compliance, random_string};
    use crate::transforms::test::create_topology;
    use crate::{
        event::{
            metric::{Metric, MetricKind, MetricValue},
            Event, LogEvent, Value,
        },
        test_util,
    };

    fn format_error(error: &mlua::Error) -> String {
        match error {
            mlua::Error::CallbackError { traceback, cause } => {
                format_error(cause) + "\n" + traceback
            }
            err => err.to_string(),
        }
    }

    fn from_config(config: &str) -> crate::Result<Box<Lua>> {
        Lua::new(&toml::from_str(config).unwrap(), "transform".into()).map(Box::new)
    }

    async fn run_transform<T: Future>(
        config: &str,
        func: impl FnOnce(Sender<Event>, Arc<Mutex<Receiver<Event>>>) -> T,
    ) -> T::Output {
        test_util::trace_init();
        assert_transform_compliance(async move {
            let config = super::super::LuaConfig::V2(toml::from_str(config).unwrap());
            let (tx, rx) = mpsc::channel(1);
            let (topology, out) = create_topology(ReceiverStream::new(rx), config).await;

            let out = Arc::new(tokio::sync::Mutex::new(out));

            let result = func(tx, Arc::clone(&out)).await;

            topology.stop().await;
            assert_eq!(out.lock().await.recv().await, None);

            result
        })
        .await
    }

    async fn next_event(out: &Arc<Mutex<Receiver<Event>>>, source: &str) -> Event {
        let event = out
            .lock()
            .await
            .recv()
            .await
            .expect("Event was not received");
        assert_eq!(
            event.source_id(),
            Some(&Arc::new(ComponentKey::from(source)))
        );
        event
    }

    #[tokio::test]
    async fn lua_runs_init_hook() {
        let line1 = random_string(9);
        run_transform(
            &format!(
                r#"
            version = "2"
            hooks.init = """function (emit)
                event = {{log={{message="{line1}"}}}}
                emit(event)
            end
            """
            hooks.process = """function (event, emit)
                emit(event)
            end
            """
            "#
            ),
            |tx, out| async move {
                let line2 = random_string(9);
                tx.send(Event::Log(LogEvent::from(line2.as_str())))
                    .await
                    .unwrap();
                drop(tx);
                assert_eq!(
                    next_event(&out, "transform").await.as_log()["message"],
                    line1.into()
                );
                assert_eq!(
                    next_event(&out, "in").await.as_log()["message"],
                    line2.into(),
                );
            },
        )
        .await;
    }

    #[tokio::test]
    async fn lua_add_field() {
        run_transform(
            r#"
            version = "2"
            hooks.process = """function (event, emit)
                event["log"]["hello"] = "goodbye"
                emit(event)
            end
            """
            "#,
            |tx, out| async move {
                let event = Event::Log(LogEvent::from("program me"));
                tx.send(event).await.unwrap();

                assert_eq!(
                    next_event(&out, "in").await.as_log()["hello"],
                    "goodbye".into()
                );
            },
        )
        .await;
    }

    #[tokio::test]
    async fn lua_read_field() {
        run_transform(
            r#"
            version = "2"
            hooks.process = """function (event, emit)
                _, _, name = string.find(event.log.message, "Hello, my name is (%a+).")
                event.log.name = name
                emit(event)
            end
            """
            "#,
            |tx, out| async move {
                let event = Event::Log(LogEvent::from("Hello, my name is Bob."));
                tx.send(event).await.unwrap();

                assert_eq!(next_event(&out, "in").await.as_log()["name"], "Bob".into());
            },
        )
        .await;
    }

    #[tokio::test]
    async fn lua_remove_field() {
        run_transform(
            r#"
            version = "2"
            hooks.process = """function (event, emit)
                event.log.name = nil
                emit(event)
            end
            """
            "#,
            |tx, out| async move {
                let mut event = LogEvent::default();
                event.insert("name", "Bob");

                tx.send(event.into()).await.unwrap();

                assert_eq!(next_event(&out, "in").await.as_log().get("name"), None);
            },
        )
        .await;
    }

    #[tokio::test]
    async fn lua_drop_event() {
        run_transform(
            r#"
            version = "2"
            hooks.process = """function (event, emit)
                -- emit nothing
            end
            """
            "#,
            |tx, _out| async move {
                let event = LogEvent::default().into();
                tx.send(event).await.unwrap();

                // "run_transform" will assert that the output stream is empty
            },
        )
        .await;
    }

    #[tokio::test]
    async fn lua_duplicate_event() {
        run_transform(
            r#"
            version = "2"
            hooks.process = """function (event, emit)
                emit(event)
                emit(event)
            end
            """
            "#,
            |tx, out| async move {
                let mut event = LogEvent::default();
                event.insert("host", "127.0.0.1");
                tx.send(event.into()).await.unwrap();

                assert!(out.lock().await.recv().await.is_some());
                assert!(out.lock().await.recv().await.is_some());
            },
        )
        .await;
    }

    #[tokio::test]
    async fn lua_read_empty_field() {
        run_transform(
            r#"
            version = "2"
            hooks.process = """function (event, emit)
                if event["log"]["non-existant"] == nil then
                  event["log"]["result"] = "empty"
                else
                  event["log"]["result"] = "found"
                end
                emit(event)
            end
            """
            "#,
            |tx, out| async move {
                let event = LogEvent::default();
                tx.send(event.into()).await.unwrap();

                assert_eq!(
                    next_event(&out, "in").await.as_log()["result"],
                    "empty".into()
                );
            },
        )
        .await;
    }

    #[tokio::test]
    async fn lua_integer_value() {
        run_transform(
            r#"
            version = "2"
            hooks.process = """function (event, emit)
                event["log"]["number"] = 3
                emit(event)
            end
            """
            "#,
            |tx, out| async move {
                let event = LogEvent::default();
                tx.send(event.into()).await.unwrap();

                assert_eq!(
                    next_event(&out, "in").await.as_log()["number"],
                    Value::Integer(3)
                );
            },
        )
        .await;
    }

    #[tokio::test]
    async fn lua_numeric_value() {
        run_transform(
            r#"
            version = "2"
            hooks.process = """function (event, emit)
                event["log"]["number"] = 3.14159
                emit(event)
            end
            """
            "#,
            |tx, out| async move {
                let event = LogEvent::default();
                tx.send(event.into()).await.unwrap();

                assert_eq!(
                    next_event(&out, "in").await.as_log()["number"],
                    Value::from(3.14159)
                );
            },
        )
        .await;
    }

    #[tokio::test]
    async fn lua_boolean_value() {
        run_transform(
            r#"
            version = "2"
            hooks.process = """function (event, emit)
                event["log"]["bool"] = true
                emit(event)
            end
            """
            "#,
            |tx, out| async move {
                let event = LogEvent::default();
                tx.send(event.into()).await.unwrap();

                assert_eq!(
                    next_event(&out, "in").await.as_log()["bool"],
                    Value::Boolean(true)
                );
            },
        )
        .await;
    }

    #[tokio::test]
    async fn lua_non_coercible_value() {
        run_transform(
            r#"
            version = "2"
            hooks.process = """function (event, emit)
                event["log"]["junk"] = nil
                emit(event)
            end
            """
            "#,
            |tx, out| async move {
                let event = LogEvent::default();
                tx.send(event.into()).await.unwrap();

                assert_eq!(next_event(&out, "in").await.as_log().get("junk"), None);
            },
        )
        .await;
    }

    #[tokio::test]
    async fn lua_non_string_key_write() -> crate::Result<()> {
        let mut transform = from_config(
            r#"
            hooks.process = """function (event, emit)
                event["log"][false] = "hello"
                emit(event)
            end
            """
            "#,
        )
        .unwrap();

        let err = transform
            .process_single(LogEvent::default().into())
            .unwrap_err();
        let err = format_error(&err);
        assert!(
            err.contains("error converting Lua boolean to String"),
            "{}",
            err
        );
        Ok(())
    }

    #[tokio::test]
    async fn lua_non_string_key_read() {
        run_transform(
            r#"
            version = "2"
            hooks.process = """function (event, emit)
                event.log.result = event.log[false]
                emit(event)
            end
            """
            "#,
            |tx, out| async move {
                let event = LogEvent::default();
                tx.send(event.into()).await.unwrap();

                assert_eq!(next_event(&out, "in").await.as_log().get("result"), None);
            },
        )
        .await;
    }

    #[tokio::test]
    async fn lua_script_error() -> crate::Result<()> {
        let mut transform = from_config(
            r#"
            hooks.process = """function (event, emit)
                error("this is an error")
            end
            """
            "#,
        )
        .unwrap();

        let err = transform
            .process_single(LogEvent::default().into())
            .unwrap_err();
        let err = format_error(&err);
        assert!(err.contains("this is an error"), "{}", err);
        Ok(())
    }

    #[tokio::test]
    async fn lua_syntax_error() -> crate::Result<()> {
        let err = from_config(
            r#"
            hooks.process = """function (event, emit)
                1234 = sadf <>&*!#@
            end
            """
            "#,
        )
        .map(|_| ())
        .unwrap_err()
        .to_string();

        assert!(err.contains("syntax error:"), "{}", err);
        Ok(())
    }

    #[tokio::test]
    async fn lua_load_file() {
        use std::{fs::File, io::Write};

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

        run_transform(
            &format!(
                r#"
            version = "2"
            hooks.process = """function (event, emit)
                local script2 = require("script2")
                script2.modify(event)
                emit(event)
            end
            """
            search_dirs = [{:?}]
            "#,
                dir.path().as_os_str() // This seems a bit weird, but recall we also support windows.
            ),
            |tx, out| async move {
                let event = LogEvent::default();
                tx.send(event.into()).await.unwrap();

                assert_eq!(
                    next_event(&out, "in").await.as_log()["\"new field\""],
                    "new value".into()
                );
            },
        )
        .await;
    }

    #[tokio::test]
    async fn lua_pairs() {
        run_transform(
            r#"
            version = "2"
            hooks.process = """function (event, emit)
                for k,v in pairs(event.log) do
                  event.log[k] = k .. v
                end
                emit(event)
            end
            """
            "#,
            |tx, out| async move {
                let mut event = LogEvent::default();
                event.insert("name", "Bob");
                event.insert("friend", "Alice");
                tx.send(event.into()).await.unwrap();

                let output = next_event(&out, "in").await;

                assert_eq!(output.as_log()["name"], "nameBob".into());
                assert_eq!(output.as_log()["friend"], "friendAlice".into());
            },
        )
        .await;
    }

    #[tokio::test]
    async fn lua_metric() {
        run_transform(
            r#"
            version = "2"
                hooks.process = """function (event, emit)
                event.metric.counter.value = event.metric.counter.value + 1
                emit(event)
            end
            """
            "#,
            |tx, out| async move {
                let metric = Metric::new(
                    "example counter",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 1.0 },
                );

                let mut expected = metric
                    .clone()
                    .with_value(MetricValue::Counter { value: 2.0 });
                let metadata = expected.metadata_mut();
                metadata.set_upstream_id(Arc::new(OutputId::from("transform")));
                metadata.set_source_id(Arc::new(ComponentKey::from("in")));

                tx.send(metric.into()).await.unwrap();

                assert_eq!(next_event(&out, "in").await.as_metric(), &expected);
            },
        )
        .await;
    }

    #[tokio::test]
    async fn lua_multiple_events() {
        run_transform(
            r#"
            version = "2"
            hooks.process = """function (event, emit)
                event["log"]["hello"] = "goodbye"
                emit(event)
            end
            """
            "#,
            |tx, out| async move {
                let n: usize = 10;
                let events =
                    (0..n).map(|i| Event::Log(LogEvent::from(format!("program me {}", i))));
                for event in events {
                    tx.send(event).await.unwrap();
                    assert!(out.lock().await.recv().await.is_some());
                }
            },
        )
        .await;
    }
}
