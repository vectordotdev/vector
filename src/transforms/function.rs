use crate::{
    config::{
        DataType, GenerateConfig, Input, Output, TransformConfig, TransformContext,
        TransformDescription,
    },
    schema,
    transforms::{remap::RemapConfig, Transform},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use toml::value::Value as TomlValue;
use value::Value;
use vector_common::TimeZone;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct FunctionConfig {
    /// The name of the VRL function to call.
    pub function: String,

    /// An optional list of arguments to pass to the function.
    ///
    /// If left empty, no arguments are passed in, or if a single argument is
    /// expected, the `.` query expression is provided to the first argument.
    pub arguments: Arguments,
    pub failure: FailureConfig,

    /// The field to assign the result of the function to.
    ///
    /// If undefined, the event root is used.
    pub target_field: Option<String>,

    /// Optional time zone configuration to use.
    #[serde(default)]
    pub timezone: TimeZone,
}

/// Function arguments.
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, untagged)]
pub enum Arguments {
    /// A list of unnamed (positional) function arguments.
    Array(Vec<TomlValue>),

    /// A list of named function arguments.
    Object(IndexMap<String, TomlValue>),
}

/// The failure mode configuration.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct FailureConfig {
    /// The log level at which to log the runtime error.
    ///
    /// Can be set to `LogLevel::None` to disable logging.
    ///
    /// Defaults to `LogLevel::Error`.
    #[serde(default = "default_log_level")]
    pub log: LogLevel,

    /// The result of a runtime failure of the function call.
    pub mode: FailureMode,
}

const fn default_log_level() -> LogLevel {
    LogLevel::Error
}

/// The level of a log message.
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
pub enum LogLevel {
    /// No log level, equivalent to not emitting the log.
    None,

    /// Debug level.
    Debug,

    /// Info level.
    Info,

    /// Warn level.
    Warn,

    /// Error level.
    Error,
}

/// The result of a runtime failure.
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
pub enum FailureMode {
    /// The provided static value is used if the function fails at runtime.
    Static(TomlValue),

    /// The (unmodified) event is re-routed to the provided component if the
    /// function fails at runtime.
    Reroute,

    /// The event is dropped if the function fails at runtime.
    Drop,
}

inventory::submit! {
    TransformDescription::new::<FunctionConfig>("function")
}

impl GenerateConfig for FunctionConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc::indoc! {r#"
            function = "parse_json"

            [[arguments]]
            value = "."

            [[failure]]
            log = "error"
            mode = "drop"
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "function")]
impl TransformConfig for FunctionConfig {
    async fn build(&self, ctx: &TransformContext) -> crate::Result<Transform> {
        vrl_stdlib::all()
            .into_iter()
            .find(|func| func.identifier() == &self.function)
            .ok_or_else(|| BuildError::UnknownFunction {
                ident: self.function.clone(),
                maybe: "TODO".to_owned(),
            })?;

        let arguments = match self.arguments.clone() {
            Arguments::Array(args) => args.into_iter().map(|arg| (None, arg)).collect::<Vec<_>>(),
            Arguments::Object(args) => args
                .into_iter()
                .map(|(ident, arg)| (Some(ident), arg))
                .collect::<Vec<_>>(),
        };

        let arguments = arguments
            .into_iter()
            .map(|(ident, arg)| {
                let mut buf = String::new();

                if let Some(ident) = ident {
                    buf.push_str(&format!("{ident}: "))
                }

                match Value::try_from(arg).map_err(|err| BuildError::InvalidArgument {
                    parameter: "TODO",
                    error: err.to_string(),
                })? {
                    Value::Bytes(v) => {
                        let v = String::from_utf8_lossy(&v);

                        // TODO: document / or find alternative approach
                        if v.starts_with(".") {
                            buf.push_str(&v);
                        } else {
                            buf.push('"');
                            buf.push_str(&v);
                            buf.push('"');
                        }
                    }
                    v => buf.push_str(&v.to_string()),
                };

                Ok(buf)
            })
            .collect::<Result<Vec<_>, BuildError>>()?
            .join(", ");

        let target = self.target_field.clone().unwrap_or_else(|| ".".to_owned());

        let mut source = format!("{target} = {}", &self.function);

        if !matches!(self.failure.mode, FailureMode::Static(_)) {
            source.push('!');
        }

        source.push_str(&format!("({arguments})"));

        if let FailureMode::Static(value) = &self.failure.mode {
            source.push_str(&format!(" ?? {}", value.to_string()));
        }

        let remap = RemapConfig {
            source: Some(source),
            file: None,
            timezone: self.timezone.clone(),
            drop_on_error: !matches!(self.failure.mode, FailureMode::Static(_)),
            drop_on_abort: false,
            reroute_dropped: matches!(self.failure.mode, FailureMode::Reroute),
            runtime: vrl::VrlRuntime::Ast,
        };

        remap.build(ctx).await
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn outputs(&self, _: &schema::Definition) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn transform_type(&self) -> &'static str {
        "function"
    }
}

#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("provided function `{}` is unknown, did you mean `{}`?", ident, maybe))]
    UnknownFunction {
        ident: String,
        maybe: String,
    },

    #[snafu(display("invalid argument `{}`: {}", parameter, error))]
    InvalidArgument {
        parameter: &'static str,
        error: String,
    },

    // TODO
    Compilation {
        error: String,
    },
}
