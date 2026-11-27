//! Helpers shared by the Iggy sink and source.

use std::borrow::Cow;
#[cfg(feature = "sinks-iggy")]
use std::time::Duration;

use iggy::prelude::{IggyClient, IggyError, PersonalAccessTokenClient, UserClient};
use percent_encoding::percent_decode_str;
use url::Url;

/// Matches the SDK's own binary operation timeout.
#[cfg(feature = "sinks-iggy")]
pub(crate) const RESOLVE_TIMEOUT: Duration = Duration::from_secs(30);

/// The SDK leaves certificate verification off for QUIC (`validate_certificate`)
/// and for TLS over WebSocket (`tls_validate_certificate`) unless the
/// connection string asks for it. An unauthenticated broker is not a safe
/// default, so the matching flag is added when the connection string omits it;
/// passing the flag as `false` still opts out. TCP TLS has no toggle (it always
/// verifies), and the HTTP client's transport has no TLS options either, so
/// both are left untouched.
pub(crate) fn secure_connection_string(connection_string: &str) -> Cow<'_, str> {
    let Some((scheme, rest)) = connection_string.split_once("://") else {
        return Cow::Borrowed(connection_string);
    };
    let flag = match scheme.strip_prefix("iggy+").unwrap_or(scheme) {
        "quic" => "validate_certificate",
        "ws" => "tls_validate_certificate",
        _ => return Cow::Borrowed(connection_string),
    };
    if let Some(value) = option_value(rest, flag) {
        if value == "false" {
            warn!(
                message = "The Iggy connection string disables certificate validation, this may lead to security vulnerabilities.",
            );
        }
        return Cow::Borrowed(connection_string);
    }
    let separator = if rest.contains('?') { '&' } else { '?' };
    Cow::Owned(format!("{connection_string}{separator}{flag}=true"))
}

/// Credentials carried by an HTTP connection string, already percent-decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
enum HttpCredentials {
    /// `<user>:<password>@<host>`
    UserPassword { username: String, password: String },
    /// `<personal_access_token>@<host>`
    PersonalAccessToken(String),
}

/// Log in after connecting when the connection string is an HTTP one. The
/// binary transports log in from the connection string, but the HTTP client
/// ignores credentials in it, and `connect()` succeeds without a session, so
/// the first protected call would fail with `Unauthenticated`.
pub(crate) async fn login_if_http(
    client: &IggyClient,
    connection_string: &str,
) -> Result<(), IggyError> {
    match http_credentials(connection_string) {
        Some(HttpCredentials::UserPassword { username, password }) => {
            client.login_user(&username, &password).await?;
        }
        Some(HttpCredentials::PersonalAccessToken(token)) => {
            client.login_with_personal_access_token(&token).await?;
        }
        None => {}
    }
    Ok(())
}

/// Whether the connection string authenticates over HTTP, the only transport
/// whose access token expires and must be renewed by the caller.
pub(crate) fn logs_in_over_http(connection_string: &str) -> bool {
    http_credentials(connection_string).is_some()
}

/// Whether the connection string uses the HTTP transport. Its client keeps no
/// consumer group state, so it cannot join a consumer group: the SDK refuses
/// the join with `FeatureUnavailable`.
#[cfg(feature = "sources-iggy")]
pub(crate) fn uses_http(connection_string: &str) -> bool {
    Url::parse(connection_string).is_ok_and(|url| url.scheme() == "iggy+http")
}

/// Whether the broker rejected the current credentials. The SDK never
/// refreshes an HTTP access token, so callers re-login from the connection
/// string and retry instead of failing the component permanently.
pub(crate) fn is_unauthenticated(error: &IggyError) -> bool {
    fn root_cause(error: &IggyError) -> &IggyError {
        match error {
            IggyError::ProducerSendFailed { cause, .. } => root_cause(cause),
            error => error,
        }
    }
    matches!(root_cause(error), IggyError::Unauthenticated)
}

/// The operation the SDK still retains because its outcome is unknown. Its
/// lane admits nothing else until `Client::resolve_operation` replays it or
/// `Client::abandon_operation` frees it, so recover before retrying.
#[cfg(feature = "sinks-iggy")]
pub(crate) fn retained_operation(error: &IggyError) -> Option<u64> {
    let cause = match error {
        IggyError::ProducerSendFailed { cause, .. } => &**cause,
        error => error,
    };
    match cause {
        IggyError::LaneBlocked(operation)
        | IggyError::PartitionLaneBlocked(_, operation)
        | IggyError::OperationUncertain(operation) => Some(*operation),
        _ => None,
    }
}

fn http_credentials(connection_string: &str) -> Option<HttpCredentials> {
    let url = Url::parse(connection_string).ok()?;
    if url.scheme() != "iggy+http" || url.username().is_empty() {
        return None;
    }
    // `Url` keeps the userinfo percent-encoded, so `p%40ss` must be decoded
    // before it reaches the SDK.
    let decode = |value: &str| percent_decode_str(value).decode_utf8_lossy().into_owned();
    let username = decode(url.username());
    Some(match url.password() {
        Some(password) => HttpCredentials::UserPassword {
            username,
            password: decode(password),
        },
        // A token-only connection string parses as a username with no
        // password.
        None => HttpCredentials::PersonalAccessToken(username),
    })
}

fn option_value<'a>(rest: &'a str, name: &str) -> Option<&'a str> {
    let query = rest.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then_some(value)
    })
}

#[cfg(test)]
mod tests {
    use super::{HttpCredentials, http_credentials, secure_connection_string};

    #[test]
    fn quic_verifies_certificates_by_default() {
        assert_eq!(
            secure_connection_string("iggy+quic://iggy:iggy@127.0.0.1:8090"),
            "iggy+quic://iggy:iggy@127.0.0.1:8090?validate_certificate=true"
        );
        assert_eq!(
            secure_connection_string("iggy+quic://127.0.0.1:8090?heartbeat_interval=1s"),
            "iggy+quic://127.0.0.1:8090?heartbeat_interval=1s&validate_certificate=true"
        );
    }

    #[test]
    fn explicit_certificate_choice_is_kept() {
        assert_eq!(
            secure_connection_string("iggy+quic://127.0.0.1:8090?validate_certificate=false"),
            "iggy+quic://127.0.0.1:8090?validate_certificate=false"
        );
        assert_eq!(
            secure_connection_string("iggy+ws://127.0.0.1:8090?tls_validate_certificate=false"),
            "iggy+ws://127.0.0.1:8090?tls_validate_certificate=false"
        );
        assert_eq!(
            secure_connection_string("iggy+ws://127.0.0.1:8090?tls_validate_certificate=true"),
            "iggy+ws://127.0.0.1:8090?tls_validate_certificate=true"
        );
    }

    #[test]
    fn websocket_verifies_certificates_by_default() {
        assert_eq!(
            secure_connection_string("iggy+ws://iggy:iggy@127.0.0.1:8092"),
            "iggy+ws://iggy:iggy@127.0.0.1:8092?tls_validate_certificate=true"
        );
    }

    #[test]
    fn transports_without_a_verification_flag_are_untouched() {
        // TCP rejects `tls_validate_certificate` as an unknown option, and its
        // TLS path always verifies; the HTTP transport has no TLS options.
        assert_eq!(
            secure_connection_string("iggy://iggy:iggy@127.0.0.1:8090"),
            "iggy://iggy:iggy@127.0.0.1:8090"
        );
        assert_eq!(
            secure_connection_string("iggy+tcp://iggy:iggy@127.0.0.1:8090"),
            "iggy+tcp://iggy:iggy@127.0.0.1:8090"
        );
        assert_eq!(
            secure_connection_string("iggy+http://iggy:iggy@127.0.0.1:3000"),
            "iggy+http://iggy:iggy@127.0.0.1:3000"
        );
        assert_eq!(secure_connection_string("not-a-url"), "not-a-url");
    }

    #[test]
    fn http_credentials_distinguish_tokens_from_password_pairs() {
        assert_eq!(
            http_credentials("iggy+http://iggy:iggy@127.0.0.1:3000"),
            Some(HttpCredentials::UserPassword {
                username: "iggy".into(),
                password: "iggy".into(),
            })
        );
        assert_eq!(
            http_credentials("iggy+http://token-value@127.0.0.1:3000"),
            Some(HttpCredentials::PersonalAccessToken("token-value".into()))
        );
    }

    #[test]
    fn http_credentials_are_percent_decoded() {
        assert_eq!(
            http_credentials("iggy+http://user:p%40ss@127.0.0.1:3000"),
            Some(HttpCredentials::UserPassword {
                username: "user".into(),
                password: "p@ss".into(),
            })
        );
        assert_eq!(
            http_credentials("iggy+http://tok%40n@127.0.0.1:3000"),
            Some(HttpCredentials::PersonalAccessToken("tok@n".into()))
        );
    }

    #[test]
    fn other_connection_strings_carry_no_http_credentials() {
        assert_eq!(http_credentials("iggy+http://127.0.0.1:3000"), None);
        assert_eq!(
            http_credentials("iggy+tcp://iggy:iggy@127.0.0.1:8090"),
            None
        );
        assert_eq!(http_credentials("not-a-url"), None);
    }
}
