//! Shared authentication config between components that use HTTP.
use std::{collections::HashMap, fmt};

use bytes::Bytes;
use headers::{authorization::Credentials, Authorization};
use http::{header::AUTHORIZATION, HeaderMap, HeaderValue, StatusCode};
use serde::{
    de::{Error, MapAccess, Visitor},
    Deserialize,
};
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

use super::ErrorMessage;

/// Configuration of the authentication strategy for server mode sinks and sources.
///
/// Use the HTTP authentication with HTTPS only. The authentication credentials are passed as an
/// HTTP header without any additional encryption beyond what is provided by the transport itself.
#[configurable_component(no_deser)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[configurable(metadata(docs::enum_tag_description = "The authentication strategy to use."))]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum HttpServerAuthConfig {
    /// Basic authentication.
    ///
    /// The username and password are concatenated and encoded using [base64][base64].
    ///
    /// [base64]: https://en.wikipedia.org/wiki/Base64
    Basic {
        /// The basic authentication username.
        #[configurable(metadata(docs::examples = "${USERNAME}"))]
        #[configurable(metadata(docs::examples = "username"))]
        username: String,

        /// The basic authentication password.
        #[configurable(metadata(docs::examples = "${PASSWORD}"))]
        #[configurable(metadata(docs::examples = "password"))]
        password: SensitiveString,
    },

    /// Custom authentication using VRL code.
    ///
    /// Takes in request and validates it using VRL code.
    Custom {
        /// The VRL boolean expression.
        source: String,
    },
}

// Custom deserializer implementation to default `strategy` to `basic`
impl<'de> Deserialize<'de> for HttpServerAuthConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct HttpServerAuthConfigVisitor;

        const FIELD_KEYS: [&str; 4] = ["strategy", "username", "password", "source"];

        impl<'de> Visitor<'de> for HttpServerAuthConfigVisitor {
            type Value = HttpServerAuthConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a valid authentication strategy (basic or custom)")
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
                    .map(String::as_str)
                    .unwrap_or_else(|| "basic");

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
                    "custom" => {
                        let source = fields
                            .remove("source")
                            .ok_or_else(|| Error::missing_field("source"))?;
                        Ok(HttpServerAuthConfig::Custom { source })
                    }
                    _ => Err(Error::unknown_variant(strategy, &["basic", "custom"])),
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
    pub fn build(
        &self,
        enrichment_tables: &vector_lib::enrichment::TableRegistry,
    ) -> crate::Result<HttpServerAuthMatcher> {
        match self {
            HttpServerAuthConfig::Basic { username, password } => {
                Ok(HttpServerAuthMatcher::AuthHeader(
                    Authorization::basic(username, password.inner()).0.encode(),
                    "Invalid username/password",
                ))
            }
            HttpServerAuthConfig::Custom { source } => {
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

                Ok(HttpServerAuthMatcher::Vrl { program })
            }
        }
    }
}

/// Built auth matcher with validated configuration
/// Can be used directly in a component to validate authentication in HTTP requests
#[derive(Clone, Debug)]
pub enum HttpServerAuthMatcher {
    /// Matcher for comparing exact value of Authorization header
    AuthHeader(HeaderValue, &'static str),
    /// Matcher for running VRL script for requests, to allow for custom validation
    Vrl {
        /// Compiled VRL script
        program: Program,
    },
}

impl HttpServerAuthMatcher {
    /// Compares passed headers to the matcher
    pub fn handle_auth(&self, headers: &HeaderMap<HeaderValue>) -> Result<(), ErrorMessage> {
        match self {
            HttpServerAuthMatcher::AuthHeader(expected, err_message) => {
                if let Some(header) = headers.get(AUTHORIZATION) {
                    if expected == header {
                        Ok(())
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
            HttpServerAuthMatcher::Vrl { program } => self.handle_vrl_auth(headers, program),
        }
    }

    fn handle_vrl_auth(
        &self,
        headers: &HeaderMap<HeaderValue>,
        program: &Program,
    ) -> Result<(), ErrorMessage> {
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
        let timezone = TimeZone::default();

        let result = Runtime::default().resolve(&mut target, program, &timezone);
        match result.map_err(|e| {
            warn!("Handling auth failed: {}", e);
            ErrorMessage::new(StatusCode::UNAUTHORIZED, "Auth failed".to_owned())
        })? {
            vrl::core::Value::Boolean(result) => {
                if result {
                    Ok(())
                } else {
                    Err(ErrorMessage::new(
                        StatusCode::UNAUTHORIZED,
                        "Auth failed".to_owned(),
                    ))
                }
            }
            _ => Err(ErrorMessage::new(
                StatusCode::UNAUTHORIZED,
                "Invalid return value".to_owned(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::test_util::random_string;
    use indoc::indoc;

    use super::*;

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
        let config: HttpServerAuthConfig = serde_yaml::from_str(indoc! { r#"
            username: foo
            password: bar
            "#
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
        let config: HttpServerAuthConfig = serde_yaml::from_str(indoc! { r#"
            strategy: basic
            username: foo
            password: bar
            "#
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
    fn build_basic_auth_should_always_work() {
        let basic_auth = HttpServerAuthConfig::Basic {
            username: random_string(16),
            password: random_string(16).into(),
        };

        let matcher = basic_auth.build(&Default::default());

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

        let (_, error_message) = basic_auth.build(&Default::default()).unwrap().auth_header();
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

        let (header, _) = basic_auth.build(&Default::default()).unwrap().auth_header();
        assert_eq!(
            Authorization::basic(&username, &password).0.encode(),
            header
        );
    }

    #[test]
    fn build_custom_should_fail_on_invalid_source() {
        let custom_auth = HttpServerAuthConfig::Custom {
            source: "invalid VRL source".to_string(),
        };

        assert!(custom_auth.build(&Default::default()).is_err());
    }

    #[test]
    fn build_custom_should_fail_on_non_boolean_return_type() {
        let custom_auth = HttpServerAuthConfig::Custom {
            source: indoc! {r#"
                .success = true
                .
                "#}
            .to_string(),
        };

        assert!(custom_auth.build(&Default::default()).is_err());
    }

    #[test]
    fn build_custom_should_success_on_proper_source_with_boolean_return_type() {
        let custom_auth = HttpServerAuthConfig::Custom {
            source: indoc! {r#"
                .headers.authorization == "Basic test"
                "#}
            .to_string(),
        };

        assert!(custom_auth.build(&Default::default()).is_ok());
    }

    #[test]
    fn basic_auth_matcher_should_return_401_when_missing_auth_header() {
        let basic_auth = HttpServerAuthConfig::Basic {
            username: random_string(16),
            password: random_string(16).into(),
        };

        let matcher = basic_auth.build(&Default::default()).unwrap();

        let result = matcher.handle_auth(&HeaderMap::new());

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

        let matcher = basic_auth.build(&Default::default()).unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Basic wrong"));
        let result = matcher.handle_auth(&headers);

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

        let matcher = basic_auth.build(&Default::default()).unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            Authorization::basic(&username, &password).0.encode(),
        );
        let result = matcher.handle_auth(&headers);

        assert!(result.is_ok());
    }

    #[test]
    fn custom_auth_matcher_should_return_ok_for_true_vrl_script_result() {
        let custom_auth = HttpServerAuthConfig::Custom {
            source: r#".headers.authorization == "test""#.to_string(),
        };

        let matcher = custom_auth.build(&Default::default()).unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("test"));
        let result = matcher.handle_auth(&headers);

        assert!(result.is_ok());
    }

    #[test]
    fn custom_auth_matcher_should_return_401_for_false_vrl_script_result() {
        let custom_auth = HttpServerAuthConfig::Custom {
            source: r#".headers.authorization == "test""#.to_string(),
        };

        let matcher = custom_auth.build(&Default::default()).unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("wrong value"));
        let result = matcher.handle_auth(&headers);

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

        let matcher = custom_auth.build(&Default::default()).unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("test"));
        let result = matcher.handle_auth(&headers);

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(401, error.code());
        assert_eq!("Auth failed", error.message());
    }
}
