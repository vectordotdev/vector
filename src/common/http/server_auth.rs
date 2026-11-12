//! Shared authentication config between components that use HTTP.
use std::{collections::HashMap, fmt, net::SocketAddr};

use bytes::Bytes;
use headers::{Authorization, authorization::Credentials};
use http::{HeaderMap, HeaderValue, StatusCode, header::AUTHORIZATION};
use serde::{
    Deserialize,
    de::{Error, MapAccess, Visitor},
};
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
#[configurable_component(no_deser)]
#[derive(Clone, Debug, Eq, PartialEq)]
// Keep the existing visitor; derive the schema and serialization from the accepted input forms.
#[serde(
    from = "HttpServerAuthRepr",
    into = "HttpServerAuthRepr",
    tag = "strategy",
    rename_all = "snake_case"
)]
#[configurable(metadata(docs::enum_tag_description = "The authentication strategy to use."))]
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

/// HTTP authentication input with a default basic strategy.
#[configurable_component]
#[serde(tag = "strategy", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The authentication strategy to use."))]
enum HttpServerAuthRepr {
    /// Bearer authentication.
    ///
    /// The token is matched against the `Authorization` header using the `Bearer` scheme.
    Bearer {
        /// The bearer token to match against incoming requests.
        #[configurable(metadata(docs::examples = "SECRET[backend.token]"))]
        #[configurable(metadata(docs::examples = "my-secret-token"))]
        token: SensitiveString,
        /// Accepted but ignored for this strategy.
        #[configurable(metadata(docs::hidden, serde::ignored))]
        #[serde(default, skip_serializing_if = "String::is_empty")]
        username: String,
        /// Accepted but ignored for this strategy.
        #[configurable(metadata(docs::hidden, serde::ignored))]
        #[serde(default, skip_serializing_if = "String::is_empty")]
        password: String,
        /// Accepted but ignored for this strategy.
        #[configurable(metadata(docs::hidden, serde::ignored))]
        #[serde(default, skip_serializing_if = "String::is_empty")]
        source: String,
    },
    /// Custom authentication using VRL code.
    ///
    /// Takes in request and validates it using VRL code. The VRL program must return a boolean.
    Custom {
        /// The VRL boolean expression.
        source: String,
        /// Accepted but ignored for this strategy.
        #[configurable(metadata(docs::hidden, serde::ignored))]
        #[serde(default, skip_serializing_if = "String::is_empty")]
        username: String,
        /// Accepted but ignored for this strategy.
        #[configurable(metadata(docs::hidden, serde::ignored))]
        #[serde(default, skip_serializing_if = "String::is_empty")]
        password: String,
        /// Accepted but ignored for this strategy.
        #[configurable(metadata(docs::hidden, serde::ignored))]
        #[serde(default, skip_serializing_if = "String::is_empty")]
        token: String,
    },
    /// Basic authentication.
    ///
    /// Used when strategy is omitted. The username and password are concatenated and encoded
    /// using [base64][base64].
    ///
    /// [base64]: https://en.wikipedia.org/wiki/Base64
    #[serde(untagged)]
    Basic {
        /// The authentication strategy to use.
        #[serde(default)]
        strategy: BasicStrategy,
        /// The basic authentication username.
        #[configurable(metadata(docs::examples = "${USERNAME}"))]
        #[configurable(metadata(docs::examples = "username"))]
        username: String,
        /// The basic authentication password.
        #[configurable(metadata(docs::examples = "${PASSWORD}"))]
        #[configurable(metadata(docs::examples = "password"))]
        password: SensitiveString,
        /// Accepted but ignored for this strategy.
        #[configurable(metadata(docs::hidden, serde::ignored))]
        #[serde(default, skip_serializing_if = "String::is_empty")]
        token: String,
        /// Accepted but ignored for this strategy.
        #[configurable(metadata(docs::hidden, serde::ignored))]
        #[serde(default, skip_serializing_if = "String::is_empty")]
        source: String,
    },
}

/// Basic authentication is the default when no strategy is specified.
#[configurable_component]
#[derive(Default)]
#[serde(rename_all = "snake_case")]
enum BasicStrategy {
    /// Basic authentication.
    ///
    /// The username and password are concatenated and encoded using [base64][base64].
    ///
    /// [base64]: https://en.wikipedia.org/wiki/Base64
    #[default]
    Basic,
}

impl From<HttpServerAuthConfig> for HttpServerAuthRepr {
    fn from(config: HttpServerAuthConfig) -> Self {
        match config {
            HttpServerAuthConfig::Basic { username, password } => Self::Basic {
                strategy: BasicStrategy::Basic,
                username,
                password,
                token: String::new(),
                source: String::new(),
            },
            HttpServerAuthConfig::Bearer { token } => Self::Bearer {
                token,
                username: String::new(),
                password: String::new(),
                source: String::new(),
            },
            HttpServerAuthConfig::Custom { source } => Self::Custom {
                source,
                username: String::new(),
                password: String::new(),
                token: String::new(),
            },
        }
    }
}

// Custom deserializer implementation to default `strategy` to `basic`
impl<'de> Deserialize<'de> for HttpServerAuthConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct HttpServerAuthConfigVisitor;

        const FIELD_KEYS: [&str; 5] = ["strategy", "username", "password", "token", "source"];

        impl<'de> Visitor<'de> for HttpServerAuthConfigVisitor {
            type Value = HttpServerAuthConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a valid authentication strategy (basic, bearer, or custom)")
            }

            fn visit_map<A>(self, mut map: A) -> Result<HttpServerAuthConfig, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut fields: HashMap<&str, String> = HashMap::default();

                while let Some(key) = map.next_key::<String>()? {
                    if let Some(field_index) = FIELD_KEYS.iter().position(|k| *k == key.as_str()) {
                        if fields.contains_key(FIELD_KEYS[field_index]) {
                            return Err(Error::duplicate_field(FIELD_KEYS[field_index]));
                        }
                        fields.insert(FIELD_KEYS[field_index], map.next_value()?);
                    } else {
                        return Err(Error::unknown_field(&key, &FIELD_KEYS));
                    }
                }

                // Default to "basic" if strategy is missing
                let strategy = fields
                    .get("strategy")
                    .map_or_else(|| "basic", String::as_str);

                match strategy {
                    "basic" => {
                        let username = fields
                            .remove("username")
                            .ok_or_else(|| Error::missing_field("username"))?;
                        let password = fields
                            .remove("password")
                            .ok_or_else(|| Error::missing_field("password"))?;
                        Ok(HttpServerAuthConfig::Basic {
                            username,
                            password: SensitiveString::from(password),
                        })
                    }
                    "bearer" => {
                        let token = fields
                            .remove("token")
                            .ok_or_else(|| Error::missing_field("token"))?;
                        Ok(HttpServerAuthConfig::Bearer {
                            token: SensitiveString::from(token),
                        })
                    }
                    "custom" => {
                        let source = fields
                            .remove("source")
                            .ok_or_else(|| Error::missing_field("source"))?;
                        Ok(HttpServerAuthConfig::Custom { source })
                    }
                    _ => Err(Error::unknown_variant(
                        strategy,
                        &["basic", "bearer", "custom"],
                    )),
                }
            }
        }

        deserializer.deserialize_map(HttpServerAuthConfigVisitor)
    }
}

impl HttpServerAuthConfig {
    /// Builds an auth matcher based on provided configuration.
    /// Used to validate configuration if needed, before passing it to the
    /// actual component for usage.
    ///
    /// # Errors
    ///
    /// Returns an error if the bearer token is not a valid `Authorization` header value, or if
    /// the custom VRL program fails to compile or does not return a boolean.
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
    ///
    /// # Errors
    ///
    /// Returns an error if the bearer token is not a valid `Authorization` header value, or if
    /// the custom VRL program fails to compile or does not return a boolean.
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
    ///
    /// # Errors
    ///
    /// Returns an [`ErrorMessage`] with status `401 Unauthorized` if an `AuthHeader` matcher finds
    /// the `Authorization` header missing or not matching, or if a `Vrl` matcher's program fails at
    /// runtime, returns `false`, or returns a non-boolean value.
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
    use serde_json::json;
    use vector_config::schema::generate_root_schema;

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
    fn config_schema_accepts_default_and_explicit_strategies() {
        let schema =
            serde_json::to_value(generate_root_schema::<HttpServerAuthConfig>().unwrap()).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        for (input, expected) in [
            (
                json!({"username": "user", "password": "pass", "token": "ignored", "source": "ignored"}),
                json!({"strategy": "basic", "username": "user", "password": "pass"}),
            ),
            (
                json!({"strategy": "basic", "username": "user", "password": "pass"}),
                json!({"strategy": "basic", "username": "user", "password": "pass"}),
            ),
            (
                json!({"strategy": "bearer", "token": "secret"}),
                json!({"strategy": "bearer", "token": "secret"}),
            ),
            (
                json!({"strategy": "custom", "source": "true", "username": "ignored", "password": "ignored", "token": "ignored"}),
                json!({"strategy": "custom", "source": "true"}),
            ),
            (
                json!({"strategy": "bearer", "token": "secret", "username": "ignored", "password": "ignored", "source": "ignored"}),
                json!({"strategy": "bearer", "token": "secret"}),
            ),
        ] {
            validator.validate(&input).unwrap();
            let parsed: HttpServerAuthConfig = serde_json::from_value(input).unwrap();
            let encoded = serde_json::to_value(parsed).unwrap();
            assert_eq!(encoded, expected);
            validator.validate(&encoded).unwrap();
        }
    }

    #[test]
    fn config_schema_requires_credentials_for_the_selected_strategy() {
        let schema =
            serde_json::to_value(generate_root_schema::<HttpServerAuthConfig>().unwrap()).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        for input in [
            json!({}),
            json!({"username": "user"}),
            json!({"password": "pass"}),
            json!({"strategy": "basic"}),
            json!({"strategy": "bearer"}),
            json!({"strategy": "custom"}),
            json!({"strategy": "bearer", "username": "user", "password": "pass"}),
            json!({"strategy": "custom", "username": "user", "password": "pass"}),
            json!({"strategy": "unknown", "username": "user", "password": "pass"}),
            json!({"strategy": null, "username": "user", "password": "pass"}),
            json!({"username": "user", "password": null}),
            json!({"strategy": "bearer", "token": 42}),
            json!({"strategy": "bearer", "token": "secret", "password": null}),
            json!({"username": "user", "password": "pass", "token": 42}),
        ] {
            assert!(!validator.is_valid(&input), "schema accepted {input}");
            assert!(
                serde_json::from_value::<HttpServerAuthConfig>(input.clone()).is_err(),
                "deserializer accepted {input}"
            );
        }
    }

    #[test]
    fn config_rejects_unknown_duplicate_and_invalid_unused_fields() {
        const INVALID_CONFIGS: [&str; 3] = [
            indoc! {"
                username: user
                password: pass
                extra: field
            "},
            indoc! {"
                username: user
                username: other
                password: pass
            "},
            indoc! {"
                strategy: basic
                strategy: bearer
                token: secret
            "},
        ];
        for config in INVALID_CONFIGS {
            assert!(serde_yaml::from_str::<HttpServerAuthConfig>(config).is_err());
        }
        for field in ["token", "source"] {
            for value in [serde_json::Value::Null, json!(42)] {
                let input = json!({"username": "user", "password": "pass", field: value});
                assert!(serde_json::from_value::<HttpServerAuthConfig>(input).is_err());
            }
        }
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
