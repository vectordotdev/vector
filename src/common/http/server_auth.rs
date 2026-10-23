//! Shared authentication config between components that use HTTP.
use std::net::SocketAddr;

use bytes::Bytes;
use headers::{Authorization, authorization::Credentials};
use http::{HeaderMap, HeaderValue, StatusCode, header::AUTHORIZATION};
use serde::{Deserialize, de::Error};
use vector_config::configurable_component;
use vector_lib::{
    TimeZone, compile_vrl,
    enrichment::TableRegistry,
    event::{Event, LogEvent, MetricTagMode, VrlTarget},
    lookup::OwnedTargetPath,
    sensitive_string::SensitiveString,
};
use vector_vrl_metrics::MetricsStorage;
use vrl::{
    compiler::{CompilationResult, CompileConfig, Program, runtime::Runtime},
    core::Value,
    prelude::TypeState,
    value::{KeyString, ObjectMap},
};

use crate::format_vrl_diagnostics;

use super::ErrorMessage;

/// Configuration of the authentication strategy for server mode sinks and sources.
///
/// Use the HTTP authentication with HTTPS only. The authentication credentials are passed as an
/// HTTP header without any additional encryption beyond what is provided by the transport itself.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(try_from = "HttpServerAuthRepr", into = "HttpServerAuthRepr")]
pub enum HttpServerAuthConfig {
    /// Basic authentication.
    ///
    /// The username and password are concatenated and encoded using [base64][base64].
    ///
    /// [base64]: https://en.wikipedia.org/wiki/Base64
    Basic {
        /// The basic authentication username.
        username: String,

        /// The basic authentication password.
        password: SensitiveString,
    },

    /// Bearer authentication.
    ///
    /// The token is matched against the `Authorization` header using the `Bearer` scheme.
    Bearer {
        /// The bearer token to match against incoming requests.
        token: SensitiveString,
    },

    /// Custom authentication using VRL code.
    ///
    /// Takes in request and validates it using VRL code. The VRL program must return a boolean.
    Custom {
        /// The VRL boolean expression.
        source: String,
    },
}

/// HTTP authentication input, including the default basic strategy.
#[configurable_component]
#[derive(Default)]
#[serde(deny_unknown_fields)]
struct HttpServerAuthRepr {
    /// The authentication strategy to use.
    #[serde(default)]
    strategy: HttpServerAuthStrategy,

    /// The basic authentication username. Required for the basic strategy.
    #[configurable(metadata(docs::examples = "${USERNAME}"))]
    #[configurable(metadata(docs::examples = "username"))]
    #[serde(
        default,
        deserialize_with = "optional_string",
        skip_serializing_if = "Option::is_none"
    )]
    username: Option<String>,

    /// The basic authentication password. Required for the basic strategy.
    #[configurable(metadata(docs::examples = "${PASSWORD}"))]
    #[configurable(metadata(docs::examples = "password"))]
    #[serde(
        default,
        deserialize_with = "optional_string",
        skip_serializing_if = "Option::is_none"
    )]
    password: Option<SensitiveString>,

    /// The bearer token to match against incoming requests. Required for the bearer strategy.
    #[configurable(metadata(docs::examples = "SECRET[backend.token]"))]
    #[configurable(metadata(docs::examples = "my-secret-token"))]
    #[serde(
        default,
        deserialize_with = "optional_string",
        skip_serializing_if = "Option::is_none"
    )]
    token: Option<SensitiveString>,

    /// The VRL boolean expression. Required for the custom strategy.
    #[serde(
        default,
        deserialize_with = "optional_string",
        skip_serializing_if = "Option::is_none"
    )]
    source: Option<String>,
}

/// HTTP authentication strategy.
#[configurable_component]
#[derive(Default)]
#[serde(rename_all = "snake_case")]
enum HttpServerAuthStrategy {
    /// Basic authentication.
    ///
    /// The username and password are concatenated and encoded using [base64][base64].
    ///
    /// [base64]: https://en.wikipedia.org/wiki/Base64
    #[default]
    Basic,
    /// Bearer authentication.
    ///
    /// The token is matched against the `Authorization` header using the `Bearer` scheme.
    Bearer,
    /// Custom authentication using VRL code.
    ///
    /// Takes in request and validates it using VRL code. The VRL program must return a boolean.
    Custom,
}

// Preserve the original String deserializer for present fields; missing fields default to None.
fn optional_string<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: From<String>,
{
    String::deserialize(deserializer).map(|value| Some(T::from(value)))
}

impl TryFrom<HttpServerAuthRepr> for HttpServerAuthConfig {
    type Error = serde::de::value::Error;

    fn try_from(input: HttpServerAuthRepr) -> Result<Self, Self::Error> {
        fn required<T>(
            value: Option<T>,
            field: &'static str,
        ) -> Result<T, serde::de::value::Error> {
            value.ok_or_else(|| serde::de::value::Error::missing_field(field))
        }
        Ok(match input.strategy {
            HttpServerAuthStrategy::Basic => Self::Basic {
                username: required(input.username, "username")?,
                password: required(input.password, "password")?,
            },
            HttpServerAuthStrategy::Bearer => Self::Bearer {
                token: required(input.token, "token")?,
            },
            HttpServerAuthStrategy::Custom => Self::Custom {
                source: required(input.source, "source")?,
            },
        })
    }
}

impl From<HttpServerAuthConfig> for HttpServerAuthRepr {
    fn from(config: HttpServerAuthConfig) -> Self {
        match config {
            HttpServerAuthConfig::Basic { username, password } => Self {
                username: Some(username),
                password: Some(password),
                ..Self::default()
            },
            HttpServerAuthConfig::Bearer { token } => Self {
                strategy: HttpServerAuthStrategy::Bearer,
                token: Some(token),
                ..Self::default()
            },
            HttpServerAuthConfig::Custom { source } => Self {
                strategy: HttpServerAuthStrategy::Custom,
                source: Some(source),
                ..Self::default()
            },
        }
    }
}

impl HttpServerAuthConfig {
    /// Builds an auth matcher based on provided configuration.
    /// Used to validate configuration if needed, before passing it to the
    /// actual component for usage.
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::missing_errors_doc,
        reason = "Audit and document the existing error contracts separately from lint enforcement."
    )]
    pub fn build(
        &self,
        enrichment_tables: &vector_lib::enrichment::TableRegistry,
        metrics_storage: &MetricsStorage,
    ) -> crate::Result<HttpServerAuthMatcher> {
        match self {
            HttpServerAuthConfig::Basic { username, password } => {
                Ok(HttpServerAuthMatcher::AuthHeader(
                    Authorization::basic(username, password.inner()).0.encode(),
                    "Invalid username/password",
                ))
            }
            HttpServerAuthConfig::Bearer { token } => {
                let auth = Authorization::bearer(token.inner())
                    .map_err(|e| format!("Invalid bearer token: {e}"))?;
                Ok(HttpServerAuthMatcher::AuthHeader(
                    auth.0.encode(),
                    "Invalid token",
                ))
            }
            HttpServerAuthConfig::Custom { source } => {
                let state = TypeState::default();

                let mut config = CompileConfig::default();
                config.set_custom(enrichment_tables.clone());
                config.set_custom(metrics_storage.clone());
                // Lock the event body (.field) as read-only, but leave metadata (%field) writable
                // so the VRL program can enrich authenticated events via %field = value.
                config.set_read_only_path(OwnedTargetPath::event_root(), true);

                let CompilationResult {
                    program,
                    warnings,
                    config: _,
                } = compile_vrl(source, &vector_vrl_functions::all(), &state, config)
                    .map_err(|diagnostics| format_vrl_diagnostics(source, diagnostics))?;

                if !program.final_type_info().result.is_boolean() {
                    return Err("VRL conditions must return a boolean.".into());
                }

                if !warnings.is_empty() {
                    let warnings = format_vrl_diagnostics(source, warnings);
                    warn!(message = "VRL compilation warning.", %warnings);
                }

                Ok(HttpServerAuthMatcher::Vrl { program })
            }
        }
    }

    /// Validates the auth configuration against the given enrichment tables,
    /// compiling any custom VRL program so `vector validate --no-environment`
    /// catches syntax/type errors while resolving enrichment table names.
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::missing_errors_doc,
        reason = "Audit and document the existing error contracts separately from lint enforcement."
    )]
    pub fn validate(&self, enrichment_tables: &TableRegistry) -> crate::Result<()> {
        self.build(enrichment_tables, &MetricsStorage::default())
            .map(|_| ())
    }
}

/// Built auth matcher with validated configuration
/// Can be used directly in a component to validate authentication in HTTP requests
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum HttpServerAuthMatcher {
    /// Matcher for comparing exact value of Authorization header
    AuthHeader(HeaderValue, &'static str),
    /// Matcher for running VRL script for requests, to allow for custom validation.
    /// Metadata (`%field`) writes in the program are extracted and returned to the caller
    /// for injection into authenticated events.
    Vrl {
        /// Compiled VRL script
        program: Program,
    },
}

impl HttpServerAuthMatcher {
    /// Validates the request. Returns `Ok(Some(enrichment))` when auth passes and the VRL program
    /// wrote `%field` values; returns `Ok(None)` when auth passes with no metadata enrichment.
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::missing_errors_doc,
        reason = "Audit and document the existing error contracts separately from lint enforcement."
    )]
    pub fn handle_auth(
        &self,
        address: Option<&SocketAddr>,
        headers: &HeaderMap<HeaderValue>,
        path: &str,
    ) -> Result<Option<ObjectMap>, ErrorMessage> {
        match self {
            HttpServerAuthMatcher::AuthHeader(expected, err_message) => {
                if let Some(header) = headers.get(AUTHORIZATION) {
                    if expected == header {
                        Ok(None)
                    } else {
                        Err(ErrorMessage::new(
                            StatusCode::UNAUTHORIZED,
                            err_message.to_string(),
                        ))
                    }
                } else {
                    Err(ErrorMessage::new(
                        StatusCode::UNAUTHORIZED,
                        "No authorization header".to_owned(),
                    ))
                }
            }
            HttpServerAuthMatcher::Vrl { program } => {
                self.handle_vrl_auth(address, headers, path, program)
            }
        }
    }

    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::default_trait_access,
        reason = "Keep inferred defaults where concrete type names need a separate import or API cleanup."
    )]
    #[allow(
        clippy::unused_self,
        reason = "Preserve the existing method receiver and call sites during the lint rollout."
    )]
    fn handle_vrl_auth(
        &self,
        address: Option<&SocketAddr>,
        headers: &HeaderMap<HeaderValue>,
        path: &str,
        program: &Program,
    ) -> Result<Option<ObjectMap>, ErrorMessage> {
        let mut target = VrlTarget::new(
            Event::Log(LogEvent::from_map(
                ObjectMap::from([
                    (
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
                    ),
                    (
                        "address".into(),
                        address.map_or(Value::Null, |a| Value::from(a.ip().to_string())),
                    ),
                    ("path".into(), Value::from(path.to_owned())),
                ]),
                Default::default(),
            )),
            program.info(),
            MetricTagMode::Single,
        );
        let timezone = TimeZone::default();

        let result = Runtime::default().resolve(&mut target, program, &timezone);
        match result.map_err(|e| {
            warn!("Handling auth failed: {e}");
            ErrorMessage::new(StatusCode::UNAUTHORIZED, "Auth failed".to_owned())
        })? {
            vrl::core::Value::Boolean(true) => {
                let enrichment = if let VrlTarget::LogEvent(_, metadata) = &target {
                    metadata
                        .value()
                        .as_object()
                        .filter(|m| !m.is_empty())
                        .cloned()
                } else {
                    None
                };
                Ok(enrichment)
            }
            vrl::core::Value::Boolean(false) => Err(ErrorMessage::new(
                StatusCode::UNAUTHORIZED,
                "Auth failed".to_owned(),
            )),
            _ => Err(ErrorMessage::new(
                StatusCode::UNAUTHORIZED,
                "Invalid return value".to_owned(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;
    use crate::test_util::{addr::next_addr, random_string};

    impl HttpServerAuthMatcher {
        fn auth_header(self) -> (HeaderValue, &'static str) {
            match self {
                HttpServerAuthMatcher::AuthHeader(header_value, error_message) => {
                    (header_value, error_message)
                }
                HttpServerAuthMatcher::Vrl { .. } => {
                    panic!("Expected HttpServerAuthMatcher::AuthHeader")
                }
            }
        }
    }

    #[test]
    fn config_should_default_to_basic() {
        let config: HttpServerAuthConfig = serde_yaml::from_str(indoc! { r"
            username: foo
            password: bar
            "
        })
        .unwrap();

        if let HttpServerAuthConfig::Basic { username, password } = config {
            assert_eq!(username, "foo");
            assert_eq!(password.inner(), "bar");
        } else {
            panic!("Expected HttpServerAuthConfig::Basic");
        }
    }

    #[test]
    fn config_should_support_explicit_basic_strategy() {
        let config: HttpServerAuthConfig = serde_yaml::from_str(indoc! { r"
            strategy: basic
            username: foo
            password: bar
            "
        })
        .unwrap();

        if let HttpServerAuthConfig::Basic { username, password } = config {
            assert_eq!(username, "foo");
            assert_eq!(password.inner(), "bar");
        } else {
            panic!("Expected HttpServerAuthConfig::Basic");
        }
    }

    #[test]
    fn config_should_support_custom_strategy() {
        let config: HttpServerAuthConfig = serde_yaml::from_str(indoc! { r#"
            strategy: custom
            source: "true"
            "#
        })
        .unwrap();

        assert!(matches!(config, HttpServerAuthConfig::Custom { .. }));
        if let HttpServerAuthConfig::Custom { source } = config {
            assert_eq!(source, "true");
        } else {
            panic!("Expected HttpServerAuthConfig::Custom");
        }
    }

    #[test]
    fn config_rejects_invalid_or_missing_auth_fields() {
        for config in [
            "username: user",
            "strategy: bearer",
            "strategy: custom",
            "strategy: unknown",
            "username: user\npassword: pass\nextra: field",
            "username: user\nusername: other\npassword: pass",
        ] {
            assert!(
                serde_yaml::from_str::<HttpServerAuthConfig>(config).is_err(),
                "{config}"
            );
        }
        // The configuration parser supplies typed values to Serde. Present null
        // or numeric fields must not acquire Option's missing-field behavior.
        for value in [serde_json::Value::Null, serde_json::json!(42)] {
            for field in ["username", "password", "token", "source"] {
                let mut config = serde_json::json!({"strategy": "bearer", "token": "secret"});
                config[field] = value.clone();
                assert!(
                    serde_json::from_value::<HttpServerAuthConfig>(config).is_err(),
                    "{field}: {value}"
                );
            }
        }
    }

    #[test]
    fn config_ignores_recognized_fields_for_other_strategies() {
        const CONFIG: &str = indoc! {r"
            strategy: bearer
            token: secret
            username: unused
            password: unused
            source: unused
        "};
        let config: HttpServerAuthConfig = serde_yaml::from_str(CONFIG).unwrap();
        assert_eq!(
            serde_json::to_value(config).unwrap(),
            serde_json::json!({
                "strategy": "bearer", "token": "secret"
            })
        );
    }

    #[test]
    fn build_basic_auth_should_always_work() {
        let basic_auth = HttpServerAuthConfig::Basic {
            username: random_string(16),
            password: random_string(16).into(),
        };

        let matcher = basic_auth.build(&TableRegistry::default(), &MetricsStorage::default());

        assert!(matcher.is_ok());
        assert!(matches!(
            matcher.unwrap(),
            HttpServerAuthMatcher::AuthHeader { .. }
        ));
    }

    #[test]
    fn build_basic_auth_should_use_username_password_related_message() {
        let basic_auth = HttpServerAuthConfig::Basic {
            username: random_string(16),
            password: random_string(16).into(),
        };

        let (_, error_message) = basic_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap()
            .auth_header();
        assert_eq!("Invalid username/password", error_message);
    }

    #[test]
    fn build_basic_auth_should_use_encode_basic_header() {
        let username = random_string(16);
        let password = random_string(16);
        let basic_auth = HttpServerAuthConfig::Basic {
            username: username.clone(),
            password: password.clone().into(),
        };

        let (header, _) = basic_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap()
            .auth_header();
        assert_eq!(
            Authorization::basic(&username, &password).0.encode(),
            header
        );
    }

    #[test]
    fn config_should_support_bearer_strategy() {
        let config: HttpServerAuthConfig = serde_yaml::from_str(indoc! { r"
            strategy: bearer
            token: my-secret-token
            "
        })
        .unwrap();

        if let HttpServerAuthConfig::Bearer { token } = config {
            assert_eq!(token.inner(), "my-secret-token");
        } else {
            panic!("Expected HttpServerAuthConfig::Bearer");
        }
    }

    #[test]
    fn build_bearer_auth_should_always_work() {
        let bearer_auth = HttpServerAuthConfig::Bearer {
            token: random_string(16).into(),
        };

        let matcher = bearer_auth.build(&TableRegistry::default(), &MetricsStorage::default());

        assert!(matcher.is_ok());
        assert!(matches!(
            matcher.unwrap(),
            HttpServerAuthMatcher::AuthHeader { .. }
        ));
    }

    #[test]
    fn build_bearer_auth_should_use_token_related_message() {
        let bearer_auth = HttpServerAuthConfig::Bearer {
            token: random_string(16).into(),
        };

        let (_, error_message) = bearer_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap()
            .auth_header();
        assert_eq!("Invalid token", error_message);
    }

    #[test]
    fn bearer_auth_matcher_should_return_401_when_missing_auth_header() {
        let bearer_auth = HttpServerAuthConfig::Bearer {
            token: "my-token".to_string().into(),
        };

        let matcher = bearer_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap();

        let (_guard, addr) = next_addr();
        let result = matcher.handle_auth(Some(&addr), &HeaderMap::new(), "/");

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(401, error.code());
        assert_eq!("No authorization header", error.message());
    }

    #[test]
    fn bearer_auth_matcher_should_return_401_with_wrong_token() {
        let bearer_auth = HttpServerAuthConfig::Bearer {
            token: "my-token".to_string().into(),
        };

        let matcher = bearer_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer wrong-token"),
        );
        let (_guard, addr) = next_addr();
        let result = matcher.handle_auth(Some(&addr), &headers, "/");

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(401, error.code());
        assert_eq!("Invalid token", error.message());
    }

    #[test]
    fn bearer_auth_matcher_should_return_ok_for_correct_token() {
        let token = "my-secret-token";
        let bearer_auth = HttpServerAuthConfig::Bearer {
            token: token.to_string().into(),
        };

        let matcher = bearer_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            Authorization::bearer(token).unwrap().0.encode(),
        );
        let (_guard, addr) = next_addr();
        let result = matcher.handle_auth(Some(&addr), &headers, "/");

        assert!(result.is_ok());
    }

    #[test]
    fn build_custom_should_fail_on_invalid_source() {
        let custom_auth = HttpServerAuthConfig::Custom {
            source: "invalid VRL source".to_string(),
        };

        assert!(
            custom_auth
                .build(&TableRegistry::default(), &MetricsStorage::default())
                .is_err()
        );
    }

    #[test]
    fn build_custom_should_fail_on_non_boolean_return_type() {
        let custom_auth = HttpServerAuthConfig::Custom {
            source: indoc! {r"
                .success = true
                .
                "}
            .to_string(),
        };

        assert!(
            custom_auth
                .build(&TableRegistry::default(), &MetricsStorage::default())
                .is_err()
        );
    }

    #[test]
    fn build_custom_should_success_on_proper_source_with_boolean_return_type() {
        let custom_auth = HttpServerAuthConfig::Custom {
            source: indoc! {r#"
                .headers.authorization == "Basic test"
                "#}
            .to_string(),
        };

        assert!(
            custom_auth
                .build(&TableRegistry::default(), &MetricsStorage::default())
                .is_ok()
        );
    }

    #[test]
    fn basic_auth_matcher_should_return_401_when_missing_auth_header() {
        let basic_auth = HttpServerAuthConfig::Basic {
            username: random_string(16),
            password: random_string(16).into(),
        };

        let matcher = basic_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap();

        let (_guard, addr) = next_addr();
        let result = matcher.handle_auth(Some(&addr), &HeaderMap::new(), "/");

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(401, error.code());
        assert_eq!("No authorization header", error.message());
    }

    #[test]
    fn basic_auth_matcher_should_return_401_and_with_wrong_credentials() {
        let basic_auth = HttpServerAuthConfig::Basic {
            username: random_string(16),
            password: random_string(16).into(),
        };

        let matcher = basic_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Basic wrong"));
        let (_guard, addr) = next_addr();
        let result = matcher.handle_auth(Some(&addr), &headers, "/");

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(401, error.code());
        assert_eq!("Invalid username/password", error.message());
    }

    #[test]
    fn basic_auth_matcher_should_return_ok_for_correct_credentials() {
        let username = random_string(16);
        let password = random_string(16);
        let basic_auth = HttpServerAuthConfig::Basic {
            username: username.clone(),
            password: password.clone().into(),
        };

        let matcher = basic_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            Authorization::basic(&username, &password).0.encode(),
        );
        let (_guard, addr) = next_addr();
        let result = matcher.handle_auth(Some(&addr), &headers, "/");

        assert!(result.is_ok());
    }

    #[test]
    fn custom_auth_matcher_should_return_ok_for_true_vrl_script_result() {
        let custom_auth = HttpServerAuthConfig::Custom {
            source: r#".headers.authorization == "test""#.to_string(),
        };

        let matcher = custom_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("test"));
        let (_guard, addr) = next_addr();
        let result = matcher.handle_auth(Some(&addr), &headers, "/");

        assert!(result.is_ok());
    }

    #[test]
    fn custom_auth_matcher_should_be_able_to_check_address() {
        let (_guard, addr) = next_addr();
        let addr_string = addr.ip().to_string();
        let custom_auth = HttpServerAuthConfig::Custom {
            source: format!(".address == \"{addr_string}\""),
        };

        let matcher = custom_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap();

        let headers = HeaderMap::new();
        let result = matcher.handle_auth(Some(&addr), &headers, "/");

        assert!(result.is_ok());
    }

    #[test]
    fn custom_auth_matcher_should_work_with_missing_address_too() {
        let (_guard, addr) = next_addr();
        let addr_string = addr.ip().to_string();
        let custom_auth = HttpServerAuthConfig::Custom {
            source: format!(".address == \"{addr_string}\""),
        };

        let matcher = custom_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap();

        let headers = HeaderMap::new();
        let result = matcher.handle_auth(None, &headers, "/");

        assert!(result.is_err());
    }

    #[test]
    fn custom_auth_matcher_should_be_able_to_check_path() {
        let custom_auth = HttpServerAuthConfig::Custom {
            source: r#".path == "/ok""#.to_string(),
        };

        let matcher = custom_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap();

        let headers = HeaderMap::new();
        let (_guard, addr) = next_addr();
        let result = matcher.handle_auth(Some(&addr), &headers, "/ok");

        assert!(result.is_ok());
    }

    #[test]
    fn custom_auth_matcher_should_return_401_with_wrong_path() {
        let custom_auth = HttpServerAuthConfig::Custom {
            source: r#".path == "/ok""#.to_string(),
        };

        let matcher = custom_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap();

        let headers = HeaderMap::new();
        let (_guard, addr) = next_addr();
        let result = matcher.handle_auth(Some(&addr), &headers, "/bad");

        assert!(result.is_err());
    }

    #[test]
    fn custom_auth_matcher_should_return_401_for_false_vrl_script_result() {
        let custom_auth = HttpServerAuthConfig::Custom {
            source: r#".headers.authorization == "test""#.to_string(),
        };

        let matcher = custom_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("wrong value"));
        let (_guard, addr) = next_addr();
        let result = matcher.handle_auth(Some(&addr), &headers, "/");

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(401, error.code());
        assert_eq!("Auth failed", error.message());
    }

    #[test]
    fn custom_auth_matcher_should_return_401_for_failed_script_execution() {
        let custom_auth = HttpServerAuthConfig::Custom {
            source: "abort".to_string(),
        };

        let matcher = custom_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("test"));
        let (_guard, addr) = next_addr();
        let result = matcher.handle_auth(Some(&addr), &headers, "/");

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(401, error.code());
        assert_eq!("Auth failed", error.message());
    }

    // Backward-compat: existing `custom` scripts that don't write metadata still work and return
    // Ok(None) — no enrichment, no change in behavior.
    #[test]
    fn custom_auth_matcher_returns_none_enrichment_when_no_metadata_written() {
        let custom_auth = HttpServerAuthConfig::Custom {
            source: r#".headers.authorization == "Bearer token""#.to_string(),
        };

        let matcher = custom_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer token"));
        let (_guard, addr) = next_addr();
        let result = matcher.handle_auth(Some(&addr), &headers, "/");

        assert!(result.is_ok());
        assert_eq!(
            None,
            result.unwrap(),
            "no metadata written => no enrichment"
        );
    }

    // Existing `custom` scripts that write metadata via `%field = value` now enrich events.
    #[test]
    fn custom_auth_matcher_returns_enrichment_when_metadata_written() {
        let custom_auth = HttpServerAuthConfig::Custom {
            source: indoc! {r#"
                %tenant_id = "acme"
                true
                "#}
            .to_string(),
        };

        let matcher = custom_auth
            .build(&TableRegistry::default(), &MetricsStorage::default())
            .unwrap();

        let headers = HeaderMap::new();
        let (_guard, addr) = next_addr();
        let result = matcher.handle_auth(Some(&addr), &headers, "/");

        assert!(result.is_ok());
        let enrichment = result.unwrap().expect("expected enrichment map");
        assert_eq!(
            enrichment.get("tenant_id").cloned(),
            Some(vrl::core::Value::from("acme")),
        );
    }

    // Existing `custom` scripts still cannot mutate event body fields.
    #[test]
    fn custom_auth_build_fails_when_event_body_write_attempted() {
        let custom_auth = HttpServerAuthConfig::Custom {
            source: indoc! {r#"
                .new_field = "value"
                true
                "#}
            .to_string(),
        };

        assert!(
            custom_auth
                .build(&TableRegistry::default(), &MetricsStorage::default())
                .is_err(),
            "writing to event body (.field) must be rejected at compile time"
        );
    }
}
