use std::{fmt, str::FromStr};

use http::uri::{Authority, PathAndQuery, Scheme, Uri};
use percent_encoding::percent_decode_str;
use snafu::{ResultExt, Snafu};
use vector_lib::configurable::configurable_component;

use crate::http::Auth;

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
    let mut url = url::Url::parse(&format!("http://{authority}"))?;

    let user = url.username();
    if !user.is_empty() {
        let user = percent_decode_str(user).decode_utf8_lossy().into_owned();

        let password = url.password().unwrap_or("");
        let password = percent_decode_str(password)
            .decode_utf8_lossy()
            .into_owned();

        // These methods have the same failure condition as `username`,
        // because we have a non-empty username, they cannot fail here.
        url.set_username("")
            .map_err(|_| "unexpected empty authority")?;
        url.set_password(None)
            .map_err(|_| "unexpected empty authority")?;

        let authority = Uri::from_maybe_shared(String::from(url))?
            .authority()
            .ok_or_else(|| "unexpected empty authority".to_string())?
            .clone();

        Ok((
            authority,
            Some(Auth::Basic {
                user,
                password: password.into(),
            }),
        ))
    } else {
        Ok((authority.clone(), None))
    }
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
#[configurable(title = "An absolute http(s) URL.", description = "")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[serde(try_from = "String", into = "String")]
pub struct HttpEndpoint(Uri);

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
    /// Requires `uri` to be an absolute `http`/`https` URL.
    pub fn new(uri: Uri) -> Result<Self, HttpEndpointError> {
        if matches!(uri.scheme_str(), Some("http" | "https")) && uri.authority().is_some() {
            Ok(Self(uri))
        } else {
            Err(HttpEndpointError::NotAbsoluteHttp {
                endpoint: uri.to_string(),
            })
        }
    }

    /// Parses `endpoint` and requires it to be an absolute `http`/`https` URL.
    pub fn parse(endpoint: &str) -> Result<Self, HttpEndpointError> {
        let uri = endpoint
            .parse::<Uri>()
            .context(InvalidUriSnafu { endpoint })?;
        Self::new(uri)
    }

    /// Returns the underlying `Uri`.
    pub const fn as_uri(&self) -> &Uri {
        &self.0
    }

    /// Consumes the endpoint, returning the underlying `Uri`.
    pub fn into_uri(self) -> Uri {
        self.0
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
            format!("{base_path}{}", path.trim_start_matches('/'))
        } else {
            format!("{base_path}/{path}")
        };
        parts.path_and_query = Some(joined.parse::<PathAndQuery>().context(InvalidPathSnafu {
            endpoint: self.0.to_string(),
            path: joined,
        })?);
        let uri = Uri::from_parts(parts).context(InvalidUriPartsSnafu {
            endpoint: self.0.to_string(),
        })?;
        Self::new(uri)
    }
}

impl fmt::Display for HttpEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn http_endpoint_rejects_non_absolute_http_urls() {
        for endpoint in [
            // No scheme: `http::Uri` parses these as authority-form or as a path.
            "example.com:8088",
            "localhost:8080",
            "/services/collector",
            "",
            // Absolute, but not a scheme `HttpClient` can dial.
            "gopher://example.com",
            "unix:///var/run/vector.sock",
            // Scheme but no authority.
            "http:///path",
        ] {
            assert!(
                matches!(
                    HttpEndpoint::parse(endpoint),
                    Err(HttpEndpointError::NotAbsoluteHttp { .. })
                        | Err(HttpEndpointError::InvalidUri { .. })
                ),
                "expected `{endpoint}` to be rejected"
            );
        }
    }

    #[test]
    fn http_endpoint_reports_unparseable_endpoints() {
        let error = HttpEndpoint::parse("http://exa mple.com").unwrap_err();
        assert!(matches!(error, HttpEndpointError::InvalidUri { .. }));
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
    }
}
