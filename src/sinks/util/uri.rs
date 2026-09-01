use std::{fmt, str::FromStr};

use http::uri::{Authority, PathAndQuery, Scheme, Uri};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use snafu::{ResultExt, Snafu};
use vector_lib::configurable::configurable_component;

use crate::http::Auth;

/// Characters that must be percent-encoded in a URI userinfo.
/// RFC 3986 unreserved characters are left as-is.
const USERINFO: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// A wrapper for `http::Uri` that implements `Deserialize` and `Serialize`.
///
/// Authorization credentials, if they exist, will be removed from the URI and stored separately in `auth`.
#[configurable_component]
#[configurable(title = "The URI component of a request.", description = "")]
#[derive(Default, Debug, Clone)]
#[serde(try_from = "String", into = "String")]
pub struct UriSerde {
    pub uri: Uri,
    pub auth: Option<Auth>,
}

impl UriSerde {
    /// `Uri` supports incomplete URIs such as "/test", "example.com", etc.
    /// This function fills in empty scheme with HTTP,
    /// and empty authority with "127.0.0.1".
    pub fn with_default_parts(&self) -> Self {
        let mut parts = self.uri.clone().into_parts();
        if parts.scheme.is_none() {
            parts.scheme = Some(Scheme::HTTP);
        }
        if parts.authority.is_none() {
            parts.authority = Some(Authority::from_static("127.0.0.1"));
        }
        if parts.path_and_query.is_none() {
            // just an empty `path_and_query`,
            // but `from_parts` will fail without this.
            parts.path_and_query = Some(PathAndQuery::from_static(""));
        }
        let uri = Uri::from_parts(parts).expect("invalid parts");
        Self {
            uri,
            auth: self.auth.clone(),
        }
    }

    /// Creates a new instance of `UriSerde` by appending a path to the existing one.
    pub fn append_path(&self, path: &str) -> crate::Result<Self> {
        let uri = self.uri.to_string();
        let self_path = uri.trim_end_matches('/');
        let other_path = path.trim_start_matches('/');
        let path = format!("{self_path}/{other_path}");
        let uri = path.parse::<Uri>()?;
        Ok(Self {
            uri,
            auth: self.auth.clone(),
        })
    }

    #[allow(clippy::missing_const_for_fn)] // constant functions cannot evaluate destructors
    pub fn with_auth(mut self, auth: Option<Auth>) -> Self {
        self.auth = auth;
        self
    }
}

impl TryFrom<String> for UriSerde {
    type Error = crate::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().parse()
    }
}

impl From<UriSerde> for String {
    fn from(uri: UriSerde) -> Self {
        uri.to_string()
    }
}

impl fmt::Display for UriSerde {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.uri.authority(), &self.auth) {
            (Some(authority), Some(Auth::Basic { user, password })) => {
                let user = utf8_percent_encode(user, USERINFO);
                let password = utf8_percent_encode(password.inner(), USERINFO);
                let authority = format!("{user}:{password}@{authority}");
                let authority =
                    Authority::from_maybe_shared(authority).map_err(|_| std::fmt::Error)?;
                let mut parts = self.uri.clone().into_parts();
                parts.authority = Some(authority);
                Uri::from_parts(parts).unwrap().fmt(f)
            }
            _ => self.uri.fmt(f),
        }
    }
}

impl FromStr for UriSerde {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let uri: Uri = s.parse()?;
        uri.try_into()
    }
}

impl TryFrom<Uri> for UriSerde {
    type Error = crate::Error;

    /// Fallible construction from a parsed `Uri`, extracting any basic auth
    /// credentials from the authority.
    ///
    /// This can fail: `http::Uri` accepts authorities that `url::Url` rejects
    /// (e.g. a non-numeric port), in which case the basic auth cannot be
    /// extracted.
    fn try_from(uri: Uri) -> Result<Self, Self::Error> {
        match uri.authority() {
            None => Ok(Self { uri, auth: None }),
            Some(authority) => {
                let (authority, auth) = get_basic_auth(authority)?;

                let mut parts = uri.into_parts();
                parts.authority = Some(authority);
                let uri = Uri::from_parts(parts)?;

                Ok(Self { uri, auth })
            }
        }
    }
}

fn get_basic_auth(authority: &Authority) -> crate::Result<(Authority, Option<Auth>)> {
    // `http::Uri` accepts authorities that `url::Url` rejects (e.g. a
    // non-numeric port), so this parse can fail; propagate the error instead
    // of panicking.
    let url = url::Url::parse(&format!("http://{authority}"))?;
    let Some((_, host_port)) = authority.as_str().rsplit_once('@') else {
        return Ok((authority.clone(), None));
    };

    let has_auth = !url.username().is_empty() || url.password().is_some();
    let auth = has_auth.then(|| {
        let user = percent_decode_str(url.username())
            .decode_utf8_lossy()
            .into_owned();
        let password = percent_decode_str(url.password().unwrap_or(""))
            .decode_utf8_lossy()
            .into_owned();
        Auth::Basic {
            user,
            password: password.into(),
        }
    });

    // Rebuild the authority from the parsed URL so the host is normalized
    // (e.g. host case), while retaining an explicit port from the raw
    // authority (`url::Url` drops the port when it matches the scheme default).
    let host = url
        .host_str()
        .ok_or_else(|| "unexpected empty authority".to_string())?;
    let port = host_port
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok());
    let authority = match port {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    };

    Ok((authority.parse()?, auth))
}

/// Simplify the URI into a protocol and endpoint by removing the
/// "query" portion of the `path_and_query`.
pub fn protocol_endpoint(uri: Uri) -> (String, String) {
    let mut parts = uri.into_parts();

    // Drop any username and password
    parts.authority = parts.authority.map(|auth| {
        let host = auth.host();
        match auth.port() {
            None => host.to_string(),
            Some(port) => format!("{host}:{port}"),
        }
        .parse()
        .unwrap_or_else(|_| unreachable!())
    });

    // Drop the query and fragment
    parts.path_and_query = parts.path_and_query.map(|pq| {
        pq.path()
            .parse::<PathAndQuery>()
            .unwrap_or_else(|_| unreachable!())
    });

    (
        parts.scheme.clone().unwrap_or(Scheme::HTTP).as_str().into(),
        Uri::from_parts(parts)
            .unwrap_or_else(|_| unreachable!())
            .to_string(),
    )
}

/// Error returned when a configured endpoint cannot be used as an absolute HTTP URL.
#[derive(Debug, Snafu)]
pub enum HttpEndpointError {
    #[snafu(display("endpoint `{endpoint}` is not a valid URI: {source}"))]
    InvalidUri {
        endpoint: String,
        source: http::uri::InvalidUri,
    },

    #[snafu(display("endpoint `{endpoint}` has an invalid path `{path}`: {source}"))]
    InvalidPath {
        endpoint: String,
        path: String,
        source: http::uri::InvalidUri,
    },

    #[snafu(display("endpoint `{endpoint}` cannot be reassembled from its parts: {source}"))]
    InvalidUriParts {
        endpoint: String,
        source: http::uri::InvalidUriParts,
    },

    #[snafu(display(
        "endpoint must be an absolute http(s) URL, for example `https://example.com`; got `{endpoint}`"
    ))]
    NotAbsoluteHttp { endpoint: String },

    #[snafu(display("endpoint `{endpoint}` has an invalid port"))]
    InvalidPort { endpoint: String },
}

/// A `Uri` proven to be an absolute `http`/`https` URL.
///
/// Constructing an `HttpEndpoint` is the only way to obtain one: both
/// [`HttpEndpoint::new`] and [`HttpEndpoint::parse`] reject URIs without an
/// `http`/`https` scheme or without an authority. Sinks that issue requests
/// through `HttpClient` need this invariant, since `HttpClient` rejects such
/// URIs at request time, deferring a pure configuration error to runtime.
///
/// As a configuration type it deserializes from a string, so an invalid
/// endpoint is rejected at config load time with the config path in the error.
///
/// Path composition goes through [`HttpEndpoint::append_path`], which
/// manipates `Uri` parts directly instead of string-concatenating and
/// re-parsing, so the scheme and authority are preserved and the result is
/// still an absolute `http(s)` URL.
#[configurable_component]
#[configurable(title = "An absolute HTTP(S) URL.", description = "")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[serde(try_from = "String", into = "String")]
pub struct HttpEndpoint(Uri);

fn redact_uri(uri: &Uri) -> String {
    if uri
        .authority()
        .is_some_and(|authority| authority.as_str().contains('@'))
    {
        "<redacted endpoint>".to_owned()
    } else {
        uri.to_string()
    }
}

/// Redacts credentials from an endpoint string for error messages.
///
/// Redacts the whole endpoint when it may carry credentials: userinfo in the
/// authority (`@`) or a `password` query parameter, which some backends (for
/// example PostgreSQL) accept as an alternative to userinfo.
pub(crate) fn redact_unparsed_endpoint(endpoint: &str) -> String {
    if endpoint.contains('@') || has_password_query_param(endpoint) {
        "<redacted endpoint>".to_owned()
    } else {
        endpoint.to_owned()
    }
}

/// Returns `true` if the query portion of `endpoint` contains a `password`
/// parameter (for example `postgres://host/db?password=secret`). Query keys
/// are percent-decoded, matching how SQLx parses them.
fn has_password_query_param(endpoint: &str) -> bool {
    endpoint.split_once('?').is_some_and(|(_, query)| {
        query.split('&').any(|pair| {
            pair.split_once('=')
                .is_some_and(|(key, _)| percent_decode_str(key).decode_utf8_lossy() == "password")
        })
    })
}

impl TryFrom<String> for HttpEndpoint {
    type Error = HttpEndpointError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<HttpEndpoint> for String {
    fn from(value: HttpEndpoint) -> Self {
        value.to_string()
    }
}

impl HttpEndpoint {
    /// Requires `uri` to be an absolute `http`/`https` URL with a host and a
    /// usable port.
    ///
    /// The authority check alone is not enough: `http://:8080` parses as a
    /// valid `http::Uri` with an authority but an empty host, and
    /// `http://localhost:notaport` parses with a nonempty host but a port that
    /// cannot be dialed. Both are checked explicitly.
    pub fn new(uri: Uri) -> Result<Self, HttpEndpointError> {
        let has_valid_scheme_and_host = matches!(uri.scheme_str(), Some("http" | "https"))
            && uri.host().is_some_and(|host| !host.is_empty());
        if !has_valid_scheme_and_host {
            return Err(HttpEndpointError::NotAbsoluteHttp {
                endpoint: redact_uri(&uri),
            });
        }
        if authority_has_invalid_port(&uri) {
            return Err(HttpEndpointError::InvalidPort {
                endpoint: redact_uri(&uri),
            });
        }
        Ok(Self(uri))
    }

    /// Parses `endpoint` and requires it to be an absolute `http`/`https` URL.
    ///
    /// A missing scheme is defaulted to `https`, so `example.com:8080` becomes
    /// `https://example.com:8080`. An explicit `http`/`https` scheme is
    /// preserved. Endpoints that still lack a host after defaulting (for
    /// example `/path`) are rejected.
    pub fn parse(endpoint: &str) -> Result<Self, HttpEndpointError> {
        Self::parse_with_default_scheme(endpoint, "https")
    }

    /// Parses `endpoint` and requires it to be an absolute `http`/`https` URL.
    ///
    /// A missing scheme is defaulted to `http` (unlike [`Self::parse`], which
    /// defaults to `https`). An explicit `http`/`https` scheme is preserved.
    /// Endpoints that still lack a host after defaulting are rejected.
    pub fn parse_default_http(endpoint: &str) -> Result<Self, HttpEndpointError> {
        Self::parse_with_default_scheme(endpoint, "http")
    }

    fn parse_with_default_scheme(
        endpoint: &str,
        default_scheme: &str,
    ) -> Result<Self, HttpEndpointError> {
        // Default a missing scheme to `default_scheme`. `http::Uri` cannot
        // parse `host:port/path` without a scheme (it reads `host` as a
        // scheme), so the scheme is added up front rather than relying on
        // the parser to accept authority-form input.
        let parse = |value: &str| {
            value
                .parse::<Uri>()
                .map_err(|source| HttpEndpointError::InvalidUri {
                    endpoint: redact_unparsed_endpoint(endpoint),
                    source,
                })
        };
        let uri = if has_scheme(endpoint) {
            parse(endpoint)?
        } else {
            parse(&format!("{default_scheme}://{endpoint}"))?
        };
        Self::new(uri)
    }

    /// Returns the underlying `Uri`.
    pub const fn as_uri(&self) -> &Uri {
        &self.0
    }
    /// Returns the URI as a string, redacting any userinfo credentials so
    /// they are never written to logs.
    pub fn redacted_uri(&self) -> String {
        redact_uri(&self.0)
    }

    /// Consumes the endpoint, returning the underlying `Uri`.
    pub fn into_uri(self) -> Uri {
        self.0
    }

    /// Extracts basic-auth credentials embedded in the authority, returning a
    /// credential-free endpoint alongside the credentials.
    pub fn extract_basic_auth(self) -> crate::Result<(Self, Option<Auth>)> {
        if !self
            .as_uri()
            .authority()
            .is_some_and(|authority| authority.as_str().contains('@'))
        {
            return Ok((self, None));
        }

        let UriSerde { uri, auth } = self.into_uri().try_into()?;
        Ok((Self::new(uri)?, auth))
    }

    /// Returns the URL scheme (`http` or `https`) of this endpoint.
    ///
    /// [`HttpEndpoint::new`] guarantees the scheme is an absolute `http`/`https`
    /// scheme, so this is infallible.
    pub fn protocol(&self) -> &str {
        self.0.scheme_str().unwrap_or("https")
    }

    /// Appends `path` to this endpoint, preserving the scheme and authority.
    ///
    /// `path` may include a leading slash and a query. The existing query, if
    /// any, is dropped (as with `UriSerde::append_path`), but the scheme and
    /// authority are preserved and the result is still an absolute `http(s)` URL.
    pub fn append_path(&self, path: &str) -> Result<Self, HttpEndpointError> {
        if path.is_empty() {
            return Ok(self.clone());
        }
        let mut parts = self.0.clone().into_parts();
        let base_path = parts
            .path_and_query
            .as_ref()
            .map(PathAndQuery::path)
            .unwrap_or_default();
        let joined = if base_path.is_empty() {
            path.to_string()
        } else if base_path.ends_with('/') {
            format!("{base_path}{}", path.strip_prefix('/').unwrap_or(path))
        } else {
            format!("{base_path}/{}", path.strip_prefix('/').unwrap_or(path))
        };
        parts.path_and_query =
            Some(
                joined
                    .parse::<PathAndQuery>()
                    .with_context(|_| InvalidPathSnafu {
                        endpoint: redact_uri(&self.0),
                        path: joined,
                    })?,
            );
        let uri = Uri::from_parts(parts).with_context(|_| InvalidUriPartsSnafu {
            endpoint: redact_uri(&self.0),
        })?;
        Self::new(uri)
    }

    /// Appends `suffix` directly to the path without inserting a separator.
    ///
    /// Unlike [`HttpEndpoint::append_path`], this does not add a `/`. It is for
    /// API method suffixes that attach directly to a resource path, such as
    /// Google's `:publish` convention.
    pub fn append_raw_suffix(&self, suffix: &str) -> Result<Self, HttpEndpointError> {
        if suffix.is_empty() {
            return Ok(self.clone());
        }
        let mut parts = self.0.clone().into_parts();
        let base_path = parts
            .path_and_query
            .as_ref()
            .map(PathAndQuery::path)
            .unwrap_or_default();
        let joined = format!("{base_path}{suffix}");
        parts.path_and_query =
            Some(
                joined
                    .parse::<PathAndQuery>()
                    .with_context(|_| InvalidPathSnafu {
                        endpoint: redact_uri(&self.0),
                        path: joined,
                    })?,
            );
        let uri = Uri::from_parts(parts).with_context(|_| InvalidUriPartsSnafu {
            endpoint: redact_uri(&self.0),
        })?;
        Self::new(uri)
    }
}

impl fmt::Display for HttpEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Returns `true` if the URI's authority contains a port that is not a valid
/// `u16`.
///
/// `http::Uri` accepts non-numeric ports (for example
/// `http://localhost:notaport`), which `HttpClient` cannot dial. `Authority::port`
/// returns `None` for both a missing port and an invalid one, so the raw
/// authority is inspected instead.
fn authority_has_invalid_port(uri: &Uri) -> bool {
    let Some(authority) = uri.authority() else {
        return false;
    };
    let auth = authority.as_str();
    // Strip any userinfo (everything up to the last `@`).
    let host_port = auth
        .rsplit_once('@')
        .map(|(_, host_port)| host_port)
        .unwrap_or(auth);
    // An IPv6 host is bracketed; the port follows the closing `]`.
    let host_end = host_port.rfind(']').map_or(0, |i| i + 1);
    let Some(host_port) = host_port.get(host_end..) else {
        return false;
    };
    host_port.rfind(':').is_some_and(|i| {
        host_port
            .get(i + 1..)
            .is_some_and(|port| port.parse::<u16>().is_err())
    })
}

/// Returns `true` if `endpoint` starts with a URI scheme (`[a-zA-Z][a-zA-Z0-9+.-]*://`).
///
/// The scheme must be at the very start: a `://` later in the path or query
/// (for example `localhost:8080/write?target=http://upstream`) is not a scheme
/// marker, so the endpoint is still defaulted to `https`.
pub(crate) fn has_scheme(endpoint: &str) -> bool {
    let Some(scheme_end) = endpoint.find("://") else {
        return false;
    };
    let Some(scheme) = endpoint.get(..scheme_end) else {
        return false;
    };
    let mut chars = scheme.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn test_parse(input: &str, expected_uri: &'static str, expected_auth: Option<(&str, &str)>) {
        let UriSerde { uri, auth } = input.parse().unwrap();
        assert_eq!(uri, Uri::from_static(expected_uri));
        assert_eq!(
            auth,
            expected_auth.map(|(user, password)| {
                Auth::Basic {
                    user: user.to_owned(),
                    password: password.to_owned().into(),
                }
            })
        );
    }

    #[test]
    fn parse_endpoint() {
        test_parse(
            "http://user:pass@example.com/test",
            "http://example.com/test",
            Some(("user", "pass")),
        );

        test_parse("localhost:8080", "localhost:8080", None);

        test_parse("/api/test", "/api/test", None);

        test_parse(
            "http://user:pass;@example.com",
            "http://example.com",
            Some(("user", "pass;")),
        );

        test_parse(
            "user:pass@example.com",
            "example.com",
            Some(("user", "pass")),
        );

        test_parse("user@example.com", "example.com", Some(("user", "")));

        test_parse(
            "https://user:pass@example.com:80/api",
            "https://example.com:80/api",
            Some(("user", "pass")),
        );

        test_parse(
            "https://:secret@example.com/api",
            "https://example.com/api",
            Some(("", "secret")),
        );
    }

    #[test]
    fn parse_rejects_malformed_authority_without_panicking() {
        // `http::Uri` accepts a non-numeric port in the authority, but
        // `url::Url` rejects it. This must be a parse error, not a panic.
        let result = "http://user:pass@localhost:notaport/path".parse::<UriSerde>();
        assert!(result.is_err());
    }

    #[test]
    fn protocol_endpoint_parses_urls() {
        let parse = |uri: &str| protocol_endpoint(uri.parse().unwrap());

        assert_eq!(
            parse("http://example.com/"),
            ("http".into(), "http://example.com/".into())
        );
        assert_eq!(
            parse("https://user:pass@example.org:123/path?query"),
            ("https".into(), "https://example.org:123/path".into())
        );
        assert_eq!(
            parse("gopher://example.net:123/path?query#frag,emt"),
            ("gopher".into(), "gopher://example.net:123/path".into())
        );
    }

    #[test]
    fn http_endpoint_accepts_absolute_http_urls() {
        for endpoint in [
            "http://example.com",
            "https://example.com",
            "https://example.com:8088/services/collector",
            "http://127.0.0.1:9000/endpoint?query=1",
            "https://user:pass@example.com/path",
            // IPv6 hosts are returned bracketed (`[::1]`) and must be accepted.
            "http://[::1]:8080",
            "https://[::1]/path",
            // A missing scheme is defaulted to https.
            "example.com",
            "example.com:8088/services/collector",
            "localhost:8080",
            "[::1]:8080",
            // A `://` later in the path or query is not a scheme marker.
            "localhost:8080/write?target=http://upstream",
        ] {
            let endpoint =
                HttpEndpoint::parse(endpoint).expect("should accept absolute http(s) URL");
            assert!(matches!(
                endpoint.as_uri().scheme_str(),
                Some("http" | "https")
            ));
            assert!(endpoint.as_uri().authority().is_some());
        }
    }

    #[test]
    fn http_endpoint_extracts_basic_auth() {
        let endpoint = HttpEndpoint::parse("http://user:pass@example.com:8080/path").unwrap();
        let (endpoint, auth) = endpoint.extract_basic_auth().unwrap();

        assert_eq!(endpoint.to_string(), "http://example.com:8080/path");
        assert!(matches!(auth, Some(Auth::Basic { user, .. }) if user == "user"));
    }

    #[rstest]
    #[case::explicit_port(
        "user:pass@example.com:80/path",
        "https://example.com:80/path",
        "user",
        "pass"
    )]
    #[case::empty_username(":secret@example.com/path", "https://example.com/path", "", "secret")]
    fn http_endpoint_auth_extraction_handles_userinfo(
        #[case] endpoint: &str,
        #[case] expected_endpoint: &str,
        #[case] expected_user: &str,
        #[case] expected_password: &str,
    ) {
        let endpoint = HttpEndpoint::parse(endpoint).unwrap();
        let (endpoint, auth) = endpoint.extract_basic_auth().unwrap();

        assert_eq!(endpoint.to_string(), expected_endpoint);
        assert!(matches!(
            auth,
            Some(Auth::Basic { user, password })
                if user == expected_user && password.inner() == expected_password
        ));
    }

    #[test]
    fn http_endpoint_auth_extraction_normalizes_host() {
        let endpoint = HttpEndpoint::parse("user:pass@EXAMPLE.com/path").unwrap();
        let (endpoint, auth) = endpoint.extract_basic_auth().unwrap();

        assert_eq!(endpoint.to_string(), "https://example.com/path");
        assert!(matches!(auth, Some(Auth::Basic { user, .. }) if user == "user"));
    }

    #[test]
    fn http_endpoint_auth_extraction_round_trips_encoded_password() {
        let endpoint = HttpEndpoint::parse(":%2F@example.com/path").unwrap();
        let (endpoint, auth) = endpoint.extract_basic_auth().unwrap();

        let uri_serde = UriSerde {
            uri: endpoint.into_uri(),
            auth,
        };
        assert_eq!(uri_serde.to_string(), "https://:%2F@example.com/path");
    }

    #[test]
    fn http_endpoint_defaults_missing_scheme_to_https() {
        for endpoint in [
            "example.com",
            "example.com:8080",
            "localhost:8080/path",
            "[::1]:8080",
        ] {
            let endpoint =
                HttpEndpoint::parse(endpoint).expect("should default a missing scheme to https");
            assert_eq!(endpoint.as_uri().scheme_str(), Some("https"));
            assert!(
                endpoint
                    .as_uri()
                    .host()
                    .is_some_and(|host| !host.is_empty())
            );
        }
        // An explicit scheme is preserved.
        assert_eq!(
            HttpEndpoint::parse("http://example.com")
                .unwrap()
                .as_uri()
                .scheme_str(),
            Some("http")
        );
    }

    #[test]
    fn http_endpoint_defaults_missing_scheme_to_http() {
        for endpoint in [
            "example.com",
            "example.com:8080",
            "localhost:8080/path",
            "[::1]:8080",
        ] {
            let endpoint = HttpEndpoint::parse_default_http(endpoint)
                .expect("should default a missing scheme to http");
            assert_eq!(endpoint.as_uri().scheme_str(), Some("http"));
            assert!(
                endpoint
                    .as_uri()
                    .host()
                    .is_some_and(|host| !host.is_empty())
            );
        }
        // An explicit scheme is preserved.
        assert_eq!(
            HttpEndpoint::parse_default_http("https://example.com")
                .unwrap()
                .as_uri()
                .scheme_str(),
            Some("https")
        );
    }

    #[test]
    fn http_endpoint_rejects_non_absolute_http_urls() {
        for endpoint in [
            // No scheme and no host: `http::Uri` parses these as a path.
            "/services/collector",
            "",
            // Absolute, but not a scheme `HttpClient` can dial.
            "gopher://example.com",
            "unix:///var/run/vector.sock",
            // Scheme but no authority.
            "http:///path",
            // Authority with a port but an empty host: `http::Uri` parses this
            // with `authority() == Some` and `host() == Some("")`.
            "http://:8080",
            "http://:8080/path",
            // A non-numeric port parses with a nonempty host but cannot be dialed.
            "http://localhost:notaport",
            "https://example.com:notaport/path",
            // Multiple port separators are rejected by the URI parser.
            "http://localhost:notaport:8080",
        ] {
            assert!(
                matches!(
                    HttpEndpoint::parse(endpoint),
                    Err(HttpEndpointError::NotAbsoluteHttp { .. })
                        | Err(HttpEndpointError::InvalidUri { .. })
                        | Err(HttpEndpointError::InvalidUriParts { .. })
                        | Err(HttpEndpointError::InvalidPort { .. })
                ),
                "expected `{endpoint}` to be rejected"
            );
        }
    }

    #[test]
    fn http_endpoint_reports_unparseable_endpoints() {
        let endpoint = "http://exa mple.com";
        let error = HttpEndpoint::parse(endpoint).unwrap_err();
        assert!(matches!(error, HttpEndpointError::InvalidUri { .. }));
        assert!(error.to_string().contains(endpoint));
    }

    #[test]
    fn http_endpoint_rejects_malformed_ports() {
        for endpoint in [
            "http://localhost:notaport",
            "https://example.com:notaport/path",
        ] {
            assert!(matches!(
                HttpEndpoint::parse(endpoint),
                Err(HttpEndpointError::InvalidPort { .. })
            ));
        }
    }

    #[test]
    fn http_endpoint_errors_redact_userinfo() {
        for endpoint in [
            "http://user:secret@localhost:notaport",
            "http://user:secret@exa mple.com",
        ] {
            let message = HttpEndpoint::parse(endpoint).unwrap_err().to_string();
            assert!(message.contains("<redacted endpoint>"), "{message}");
            assert!(!message.contains("secret"), "{message}");
        }
    }

    #[test]
    fn redact_unparsed_endpoint_redacts_credentials() {
        // Userinfo in the authority.
        assert_eq!(
            redact_unparsed_endpoint("postgres://user:secret@host/db"),
            "<redacted endpoint>"
        );
        // A percent-encoded `password` query key, which SQLx decodes.
        assert_eq!(
            redact_unparsed_endpoint("postgres://host/db?pass%77ord=secret"),
            "<redacted endpoint>"
        );
        // Both forms together.
        assert_eq!(
            redact_unparsed_endpoint("postgres://user:secret@host/db?password=secret"),
            "<redacted endpoint>"
        );
        // Endpoints without credentials are left intact.
        assert_eq!(
            redact_unparsed_endpoint("postgres://host/db"),
            "postgres://host/db"
        );
        assert_eq!(
            redact_unparsed_endpoint("postgres://host/db?user=alice"),
            "postgres://host/db?user=alice"
        );
    }

    #[test]
    fn http_endpoint_append_errors_redact_userinfo() {
        let endpoint = HttpEndpoint::parse("https://user:secret@example.com/base").unwrap();

        for error in [
            endpoint.append_path("invalid path").unwrap_err(),
            endpoint.append_raw_suffix(" invalid suffix").unwrap_err(),
        ] {
            assert!(matches!(&error, HttpEndpointError::InvalidPath { .. }));
            let message = error.to_string();
            assert!(message.contains("<redacted endpoint>"), "{message}");
            assert!(!message.contains("secret"), "{message}");
        }
    }

    #[test]
    fn http_endpoint_append_path_joins_without_string_concatenation() {
        let base = HttpEndpoint::parse("https://example.com").unwrap();

        assert_eq!(
            base.append_path("vector/events").unwrap().to_string(),
            "https://example.com/vector/events"
        );
        assert_eq!(
            base.append_path("/api/v1/series").unwrap().to_string(),
            "https://example.com/api/v1/series"
        );
        assert_eq!(
            HttpEndpoint::parse("https://example.com/")
                .unwrap()
                .append_path("vector/events")
                .unwrap()
                .to_string(),
            "https://example.com/vector/events"
        );
        // The query is carried in the appended path.
        assert_eq!(
            base.append_path("/write?db=mydb").unwrap().to_string(),
            "https://example.com/write?db=mydb"
        );
        // The scheme and authority survive appending.
        let appended = HttpEndpoint::parse("https://user:pass@example.com:8088/base")
            .unwrap()
            .append_path("sub/path")
            .unwrap();
        assert_eq!(
            appended.to_string(),
            "https://user:pass@example.com:8088/base/sub/path"
        );
        assert!(matches!(appended.as_uri().scheme_str(), Some("https")));
        // A non-root base path with a leading-slash appended path must not
        // produce a double slash.
        assert_eq!(
            HttpEndpoint::parse("https://proxy/prefix")
                .unwrap()
                .append_path("/api/v1/series")
                .unwrap()
                .to_string(),
            "https://proxy/prefix/api/v1/series"
        );
        // Only the single boundary slash is removed; significant leading
        // slashes in the appended path are preserved (GCS object keys).
        assert_eq!(
            HttpEndpoint::parse("https://storage.googleapis.com/bucket/")
                .unwrap()
                .append_path("//archive/")
                .unwrap()
                .to_string(),
            "https://storage.googleapis.com/bucket//archive/"
        );
    }

    #[test]
    fn http_endpoint_append_raw_suffix_attaches_without_separator() {
        let base = HttpEndpoint::parse("https://example.com/v1/projects/p/topics/t").unwrap();
        assert_eq!(
            base.append_raw_suffix(":publish").unwrap().to_string(),
            "https://example.com/v1/projects/p/topics/t:publish"
        );
        // An empty suffix returns the endpoint unchanged.
        assert_eq!(
            base.append_raw_suffix("").unwrap().to_string(),
            base.to_string()
        );
    }
}
