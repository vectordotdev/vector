use bytes::Bytes;
use headers::{authorization::Credentials, Authorization};
use http::{header::AUTHORIZATION, HeaderMap, HeaderValue};
use vector_config::configurable_component;
use vector_lib::{
    compile_vrl,
    event::{Event, LogEvent, VrlTarget},
    sensitive_string::SensitiveString,
    TimeZone,
};
use vrl::{
    compiler::{runtime::Runtime, CompilationResult, CompileConfig, Program},
    core::Value,
    diagnostic::Formatter,
    prelude::TypeState,
    value::{KeyString, ObjectMap},
};

/// Configuration of the authentication strategy for server mode sinks and sources.
///
/// HTTP authentication should be used with HTTPS only, as the authentication credentials are passed as an
/// HTTP header without any additional encryption beyond what is provided by the transport itself.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
#[configurable(metadata(docs::enum_tag_description = "The authentication strategy to use."))]
pub enum Auth {
    /// Basic authentication.
    ///
    /// The username and password are concatenated and encoded via [base64][base64].
    ///
    /// [base64]: https://en.wikipedia.org/wiki/Base64
    Basic {
        /// The basic authentication username.
        #[configurable(metadata(docs::examples = "${USERNAME}"))]
        #[configurable(metadata(docs::examples = "username"))]
        user: String,

        /// The basic authentication password.
        #[configurable(metadata(docs::examples = "${PASSWORD}"))]
        #[configurable(metadata(docs::examples = "password"))]
        password: SensitiveString,
    },

    /// Bearer authentication.
    ///
    /// The bearer token value (OAuth2, JWT, etc.) is passed as-is.
    Bearer {
        /// The bearer authentication token.
        token: SensitiveString,
    },

    /// Custom authentication using VRL code.
    ///
    /// Takes in request and validates it using VRL code.
    Custom {
        /// The VRL boolean expression.
        source: String,
    },
}

impl Auth {
    pub fn build(
        &self,
        enrichment_tables: &vector_lib::enrichment::TableRegistry,
    ) -> crate::Result<AuthMatcher> {
        match self {
            Auth::Basic { user, password } => Ok(AuthMatcher::AuthHeader(
                Authorization::basic(user, password.inner()).0.encode(),
            )),
            Auth::Bearer { token } => Ok(AuthMatcher::AuthHeader(
                Authorization::bearer(token.inner())
                    .map_err(|_| "Invalid bearer token")?
                    .0
                    .encode(),
            )),
            Auth::Custom { source } => {
                let functions = vrl::stdlib::all()
                    .into_iter()
                    .chain(vector_lib::enrichment::vrl_functions())
                    .chain(vector_vrl_functions::all())
                    .collect::<Vec<_>>();

                let state = TypeState::default();

                let mut config = CompileConfig::default();
                config.set_custom(enrichment_tables.clone());
                config.set_read_only();

                let CompilationResult {
                    program,
                    warnings,
                    config: _,
                } = compile_vrl(source, &functions, &state, config).map_err(|diagnostics| {
                    Formatter::new(source, diagnostics).colored().to_string()
                })?;

                if !program.final_type_info().result.is_boolean() {
                    return Err("VRL conditions must return a boolean.".into());
                }

                if !warnings.is_empty() {
                    let warnings = Formatter::new(source, warnings).colored().to_string();
                    warn!(message = "VRL compilation warning.", %warnings);
                }

                Ok(AuthMatcher::Vrl { program })
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum AuthMatcher {
    AuthHeader(HeaderValue),
    Vrl { program: Program },
}

impl AuthMatcher {
    pub fn handle_auth(&self, headers: &HeaderMap<HeaderValue>) -> Result<(), String> {
        match self {
            AuthMatcher::AuthHeader(expected) => {
                if let Some(header) = headers.get(AUTHORIZATION) {
                    if expected == header {
                        Ok(())
                    } else {
                        Err("Invalid auth header.".to_string())
                    }
                } else {
                    Err("Missing auth header".to_string())
                }
            }
            AuthMatcher::Vrl { program } => {
                let mut target = VrlTarget::new(
                    Event::Log(LogEvent::from_map(
                        ObjectMap::from([(
                            "headers".into(),
                            Value::Object(
                                headers
                                    .iter()
                                    .map(|(k, v)| {
                                        (
                                            KeyString::from(k.to_string()),
                                            Value::Bytes(Bytes::copy_from_slice(v.as_bytes())),
                                        )
                                    })
                                    .collect::<ObjectMap>(),
                            ),
                        )]),
                        Default::default(),
                    )),
                    program.info(),
                    false,
                );
                // TODO: use timezone from remap config
                let timezone = TimeZone::default();

                let result = Runtime::default().resolve(&mut target, program, &timezone);
                match result.map_err(|e| {
                    warn!("Handling auth failed: {}", e);
                    "Auth failed".to_string()
                })? {
                    vrl::core::Value::Boolean(result) => {
                        if result {
                            Ok(())
                        } else {
                            Err("Auth failed".to_string())
                        }
                    }
                    _ => Err("Invalid return value".to_string()),
                }
            }
        }
    }
}
