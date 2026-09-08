#[derive(Debug, Snafu)]
#[snafu(module(build_error))]
pub(crate) enum BuildError {
    /// Template has dynamic content but no literal prefix to confine it to.
    #[snafu(display(
        "template has dynamic content (event fields: {fields:?}) but has no \
         literal string prefix to derive a confinement base from. Add a static \
         prefix to your template, or set \
         `dangerously_allow_unconfined_template_resolution: true` to opt out."
    ))]
    NoDerivableBase {
        /// The event fields referenced by the template, if any.
        fields: Vec<String>,
    },

    /// The only derivable prefix is a bare root (`/`), which would allow writes
    /// to any path under the server's namespace root.
    #[snafu(display(
        "template has only `\"/\"` as its literal prefix (from {prefix:?}), \
         which would permit writes anywhere in the namespace root. Add a \
         non-root static prefix to your template, or set \
         `dangerously_allow_unconfined_template_resolution: true` to opt out."
    ))]
    DerivedBaseIsRoot {
        /// The literal prefix that resolved to root.
        prefix: String,
    },

    /// The template is an HTTP/HTTPS URI but the static prefix ends before the
    /// authority (host + optional port), so the rendered URL's destination host
    /// is entirely event-controlled. Supply a static scheme + host, or set
    /// `dangerously_allow_unconfined_template_resolution: true` to opt out.
    #[snafu(display(
        "HTTP/HTTPS template {prefix:?} has no static authority (host): the \
         destination host would be fully event-controlled. Add a static host to \
         your URI template, or set \
         `dangerously_allow_unconfined_template_resolution: true` to opt out."
    ))]
    NoStaticUriAuthority {
        /// The literal prefix that contained no host.
        prefix: String,
    },

    /// The template has a URI scheme other than HTTP or HTTPS, which are not
    /// supported for URI confinement.
    #[snafu(display(
        "URI template {prefix:?} uses unsupported scheme {scheme:?}. \
         Only HTTP and HTTPS are supported for URI confinement, or set \
         `dangerously_allow_unconfined_template_resolution: true` to opt out."
    ))]
    UnsupportedUriScheme {
        /// The literal prefix with the unsupported scheme.
        prefix: String,
        /// The unsupported scheme.
        scheme: String,
    },

    /// The operator-authored URI prefix contains a percent-encoded path separator
    /// (`%2f` or `%5c`) or a raw backslash. These would cause every rendered
    /// event to be dropped at runtime; reject at build time instead.
    #[snafu(display(
        "HTTP/HTTPS URI prefix {prefix:?} contains %2F, %5C, or a raw backslash \
         in the static portion. Use a literal `/` in the path instead."
    ))]
    EncodedSeparatorInUriPrefix {
        /// The literal prefix that contained the encoded separator.
        prefix: String,
    },

    #[snafu(display(
        "HTTP/HTTPS template {prefix:?} has a `{{{{ field }}}}` reference inside \
         the authority (host) component: the static prefix has no `/` after \
         the host, so the rendered host is partly event-controlled. \
         Add a `/` after the static host in your URI template, or set \
         `dangerously_allow_unconfined_template_resolution: true` to opt out."
    ))]
    PartialUriAuthority {
        /// The literal prefix whose authority was left unterminated.
        prefix: String,
    },

    /// The template is an HTTP/HTTPS URI containing `?` or `#` in combination
    /// with `{{ field }}` references.
    ///
    /// A field-rendered value can inject a `?query` or a `#fragment` that
    /// steers routing or (for `#`) silently drops any operator-authored
    /// suffix through `http::Uri`'s fragment truncation.
    #[snafu(display(
        "HTTP/HTTPS template {template:?} mixes `{{{{ field }}}}` references \
         with `?` or `#`, which cannot be confined. Move event-driven routing \
         into the URL path, or set \
         `dangerously_allow_unconfined_template_resolution: true` to opt out."
    ))]
    DynamicUriQueryOrFragment {
        /// The full template source that mixed dynamic fields with `?` or `#`.
        template: String,
    },
}

#[derive(Debug, Snafu)]
#[snafu(module(confine_error))]
pub(crate) enum ConfineError {
    /// Rendered value contains a NUL byte.
    #[snafu(display("rendered value contains a NUL byte"))]
    NulByte,

    /// Rendered value exceeds the maximum allowed byte length.
    #[snafu(display("rendered value is {len} bytes; maximum allowed is {max}"))]
    TooLong {
        /// Actual length of the rendered value in bytes.
        len: usize,
        /// Maximum allowed length in bytes.
        max: usize,
    },

    /// Rendered value does not start with the required base prefix.
    #[snafu(display(
        "rendered value {:?} does not start with the base prefix {base:?}",
        confined_preview(rendered)
    ))]
    OutsideBase {
        /// The rendered value that failed confinement.
        rendered: String,
        /// The required base prefix.
        base: String,
    },

    /// Rejected because a `..` segment could escape the namespace root on
    /// filesystem-like protocols (e.g. WebHDFS) even when the string prefix
    /// check passes (e.g. `safe/../../escape` starts with `safe/`).
    #[snafu(display(
        "rendered value {:?} contains a `..` path segment",
        confined_preview(rendered)
    ))]
    DotDotSegment {
        /// The rendered value that contained the `..` segment.
        rendered: String,
    },

    /// Rendered URI could not be parsed.
    #[snafu(display("rendered value {:?} is not a valid URI", confined_preview(rendered)))]
    UriParseFailed {
        /// The rendered value that could not be parsed.
        rendered: String,
    },

    /// Rendered URI has a different scheme or authority (host + port) than the
    /// operator-configured base. This covers both `@`-userinfo injection
    /// (`trusted.host@evil.com`) and host-extension attacks
    /// (`trusted.host.evil.com`).
    #[snafu(display(
        "rendered URI {:?} has authority {actual:?} but the confined base requires {expected:?}",
        confined_preview(rendered)
    ))]
    UriAuthorityMismatch {
        /// The rendered value that failed confinement.
        rendered: String,
        /// The authority that was required.
        expected: String,
        /// The authority that was actually present.
        actual: String,
    },
}

/// Confinement checker stored on a [`Template`] at build time.
///
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ConfinementChecker {
    Prefix(PrefixChecker),
    // Boxed so the checker stays small: `UriChecker` carries several strings,
    // and `ConfinedTemplate` embeds this enum, which in turn is embedded in
    // sink configs.
    Uri(Box<UriChecker>),
}

impl ConfinementChecker {
    /// Validate common constraints for both prefix and URI confinement.
    ///
    /// Returns the literal prefix if the template is dynamic and requires
    /// confinement. Returns `Ok(None)` if the template is static and needs no
    /// confinement.
    fn validate_common(tpl: &UnconfinedTemplate) -> Result<Option<String>, BuildError> {
        let fields = match tpl.get_fields() {
            Some(f) => f,
            None => return Ok(None),
        };
        let prefix = tpl.literal_prefix();
        if prefix.is_empty() {
            return Err(BuildError::NoDerivableBase { fields });
        }
        Ok(Some(prefix.to_string()))
    }

    /// Build a prefix-based confinement checker for **non-URI fields**.
    ///
    /// Used for object-store keys, Kafka topics, Redis keys, tenant IDs, etc.
    /// The template's literal prefix becomes the confinement base. A template
    /// starting with `http://` or `https://` is treated as a non-URI string
    /// prefix (e.g., object-store key `"http://logs-{{ region }}/"`).
    ///
    /// Returns `Ok(None)` for static templates that need no confinement.
    ///
    /// Errors:
    /// - `NoDerivableBase`: template has field references but no literal prefix
    /// - `DerivedBaseIsRoot`: prefix is exactly `"/"` (trivial confinement)
    pub(crate) fn for_prefix_template(tpl: &Template) -> Result<Option<Self>, BuildError> {
        match Self::validate_common(&tpl.inner)? {
            Some(prefix) => {
                // Reject root-only prefix to avoid trivial confinement.
                if prefix == "/" {
                    return Err(BuildError::DerivedBaseIsRoot { prefix });
                }
                Ok(Some(Self::Prefix(PrefixChecker { base: prefix })))
            }
            None => Ok(None),
        }
    }

    /// Build a URI-specific confinement checker for **HTTP/HTTPS URI fields**.
    ///
    /// The template must start with `http://` or `https://` and include a static
    /// authority (host), regardless of whether it has dynamic field references.
    /// URI templates with `?` or `#` combined with field references are rejected
    /// (query/fragment injection).
    ///
    /// **All URI templates** (static and dynamic) are validated for:
    /// - HTTP/HTTPS scheme (rejects ftp://, relative paths, schemeless URIs)
    /// - Valid URI structure and non-empty authority
    ///
    /// **Static URI templates** (no `{{ }}` field references) return `Ok(None)`
    /// because they have no event-controlled content to confine at runtime.
    ///
    /// **Dynamic URI templates** return `Ok(Some(checker))` for runtime confinement.
    ///
    /// Errors:
    /// - `NoDerivableBase`: template has field references but no literal prefix
    /// - `NoStaticUriAuthority`: URI is malformed, relative, schemeless, or lacks a static host
    /// - `PartialUriAuthority`: field reference inside the authority component
    /// - `DynamicUriQueryOrFragment`: `?` or `#` with field references
    /// - `EncodedSeparatorInUriPrefix`: `%2F`, `%5C`, or backslash in prefix
    /// - `UnsupportedUriScheme`: non-HTTP(S) scheme like ftp://
    pub(crate) fn for_uri_template(tpl: &Template) -> Result<Option<Self>, BuildError> {
        match Self::validate_common(&tpl.inner)? {
            Some(prefix) => {
                // Reject URI templates that have field references AND `?` or `#`.
                // A static query/fragment is safe (fixed value, not
                // event-controlled). But once a `{{ field }}` is present, the
                // rendered path segment can smuggle either:
                //   - a `?extra=...` query string, or
                //   - a `#frag` that `http::Uri` truncates before our checker
                //     sees the path, silently dropping any operator-authored
                //     suffix like `/ingest`.
                let src = tpl.get_ref();
                if src.contains('?') || src.contains('#') {
                    return Err(BuildError::DynamicUriQueryOrFragment {
                        template: src.to_string(),
                    });
                }
                UriChecker::from_prefix(&prefix).map(|c| Some(Self::Uri(Box::new(c))))
            }
            None => {
                // Template with no event-field references. Fully static ones
                // (no strftime either) are validated for URI structure to
                // enforce the HTTP/HTTPS + authority requirement uniformly.
                // Strftime-only templates (e.g. `https://logs.example.com/%Y/%m`)
                // are not: their source is not a literal URI — the strftime
                // directives render to time-derived text, never event input —
                // and `http::Uri` would reject the raw `%` directives as
                // invalid percent escapes.
                if !tpl.inner.is_dynamic() {
                    Self::validate_static_uri(tpl).map(|()| None)
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Validate a static URI template for structure and scheme.
    ///
    /// Static templates don't need a runtime checker, but we still enforce:
    /// - Must be HTTP or HTTPS scheme
    /// - Must have valid URI structure with non-empty authority
    ///
    /// This prevents `UriTemplate::confine` from accepting `ftp://`, relative `/path`,
    /// or schemeless `//host` URIs even when static.
    fn validate_static_uri(tpl: &Template) -> Result<(), BuildError> {
        // Static templates don't need a runtime checker, but we still enforce
        // HTTP/HTTPS scheme and a non-empty authority (host) uniformly.
        UriChecker::parse_http_uri(tpl.get_ref()).map(|_| ())
    }

    pub(crate) fn confine(&self, rendered: &str) -> Result<(), ConfineError> {
        if rendered.contains('\0') {
            return Err(ConfineError::NulByte);
        }
        if rendered.len() > MAX_RENDERED_PATH_LEN {
            return Err(ConfineError::TooLong {
                len: rendered.len(),
                max: MAX_RENDERED_PATH_LEN,
            });
        }
        match self {
            Self::Prefix(c) => c.confine(rendered),
            Self::Uri(c) => c.confine(rendered),
        }
    }
}

/// Confinement for non-URI templates (Kafka topics, Redis keys, tenant IDs, …).
///
/// Enforces that the rendered value starts with the operator-controlled literal
/// prefix and contains no `..` path segments.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PrefixChecker {
    base: String,
}

impl PrefixChecker {
    pub(crate) fn confine(&self, rendered: &str) -> Result<(), ConfineError> {
        // Reject `..` segments: on filesystem-like protocols (e.g. WebHDFS) a
        // value like `safe/../../escape` passes `starts_with("safe/")` but
        // resolves outside the namespace root on the server.
        if rendered.split('/').any(|seg| seg == "..") {
            return Err(ConfineError::DotDotSegment {
                rendered: rendered.to_string(),
            });
        }
        if !rendered.starts_with(&self.base) {
            return Err(ConfineError::OutsideBase {
                rendered: rendered.to_string(),
                base: self.base.clone(),
            });
        }
        Ok(())
    }
}

/// Confinement for HTTP/HTTPS URI templates.
///
/// At build time the operator-authored static prefix is parsed with
/// `http::Uri` and its scheme, authority, and path are stored. Scheme and
/// authority are normalized to lowercase; the path prefix remains case-sensitive.
/// At render time the rendered value is also parsed with `http::Uri` and the
/// structured fields are compared, which avoids all the pitfalls of raw-string
/// heuristics (case sensitivity in scheme/authority, percent-encoding,
/// `@`-injection inside the authority component, etc.).
///
/// Build-time validation:
/// - Rejects relative URIs, schemeless URIs, and non-HTTP/HTTPS schemes
/// - Rejects templates with `?` or `#` combined with field references
/// - Rejects encoded path separators (`%2F`, `%5C`) in static prefix
/// - Requires static authority (host)
///
/// Render-time checks:
///
/// 1. **Fragment rejection** — rejects raw `#` before parsing to prevent
///    `http::Uri` truncation from hiding operator-authored suffixes.
///
/// 2. **Authority check** — scheme and authority must match the operator-authored
///    values (case-insensitive). Catches `@`-userinfo injection and host-extension.
///
/// 3. **Path-prefix check** — the rendered URI's path must start with the
///    static path portion (case-sensitive).
///
/// 4. **Dot-dot segment check** — no path segment may be `..`, `.%2e`,
///    `%2e.`, or `%2e%2e` (case-insensitive). Catches path traversal.
///
/// 5. **Encoded separator check** — rejects `%2f`, `%5c`, `%25`, and raw `\`
///    in the path (double-decoding and Windows path separator vectors).
///
/// 6. **Query rejection** — rejects any rendered query string (field-smuggled `?`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct UriChecker {
    /// Lowercased scheme, e.g. `"https"`.
    scheme: String,
    /// Lowercased authority (host + optional port), e.g. `"api.internal"`.
    authority: String,
    /// Static path portion from the template prefix, e.g. `"/ingest/"`.
    path_prefix: String,
}

impl UriChecker {
    /// Parse `prefix` as an HTTP(S) URI, validating the scheme and that a
    /// non-empty authority (host) is present. Returns the parsed `Uri` plus the
    /// lowercased scheme and authority.
    fn parse_http_uri(prefix: &str) -> Result<(Uri, String, String), BuildError> {
        let uri = prefix
            .parse::<Uri>()
            .map_err(|_| BuildError::NoStaticUriAuthority {
                prefix: prefix.to_string(),
            })?;

        // Explicitly validate HTTP/HTTPS scheme - reject relative, schemeless,
        // and non-HTTP schemes like ftp://
        let scheme = match uri.scheme_str() {
            Some(s) if s.eq_ignore_ascii_case("http") || s.eq_ignore_ascii_case("https") => {
                s.to_ascii_lowercase()
            }
            Some(s) => {
                return Err(BuildError::UnsupportedUriScheme {
                    prefix: prefix.to_string(),
                    scheme: s.to_string(),
                });
            }
            None => {
                return Err(BuildError::NoStaticUriAuthority {
                    prefix: prefix.to_string(),
                });
            }
        };

        let authority = match uri.authority() {
            Some(auth) if !auth.as_str().is_empty() => auth.as_str().to_ascii_lowercase(),
            _ => {
                return Err(BuildError::NoStaticUriAuthority {
                    prefix: prefix.to_string(),
                });
            }
        };

        Ok((uri, scheme, authority))
    }

    pub(crate) fn from_prefix(prefix: &str) -> Result<Self, BuildError> {
        let (uri, scheme, authority) = Self::parse_http_uri(prefix)?;
        let path = uri.path().to_ascii_lowercase();
        // Reject encoded path separators, encoded percents, and raw
        // backslashes in the operator-authored prefix. Any of these would
        // cause every rendered URI to fail the render-time check, silently
        // dropping all events; detect at build time instead.
        if path.contains("%2f")
            || path.contains("%5c")
            || path.contains("%25")
            || uri.path().contains('\\')
        {
            return Err(BuildError::EncodedSeparatorInUriPrefix {
                prefix: prefix.to_string(),
            });
        }
        // `http::Uri` normalises a missing path to `"/"`, so `uri.path()` can't
        // tell us whether the prefix actually had a `/` closing off the host. Check
        // the raw prefix instead: no `/` after `://` means the `{{ field }}`
        // reference sits inside (or extends) the authority we just parsed.
        let after_scheme = prefix
            .split_once("://")
            .map(|(_, rest)| rest)
            .ok_or_else(|| BuildError::NoStaticUriAuthority {
                prefix: prefix.to_string(),
            })?;
        if !after_scheme.contains('/') {
            return Err(BuildError::PartialUriAuthority {
                prefix: prefix.to_string(),
            });
        }
        Ok(Self {
            scheme,
            authority,
            path_prefix: uri.path().to_string(),
        })
    }

    pub(crate) fn confine(&self, rendered: &str) -> Result<(), ConfineError> {
        // 1. Reject raw fragment injection BEFORE parsing.
        //    `http::Uri` strips fragments server-side, which would hide the
        //    operator-authored suffix in templates like:
        //      `https://api.internal/base/{{ tenant }}/ingest`
        //    where tenant = "ok#evil" renders as:
        //      `https://api.internal/base/ok#evil`
        //    and the parser discards `/ingest` (never checked).
        //    Checking the raw string catches this before truncation.
        if rendered.contains('#') {
            return Err(ConfineError::OutsideBase {
                rendered: rendered.to_string(),
                base: format!("{}://{}{}", self.scheme, self.authority, self.path_prefix),
            });
        }

        // Parse with http::Uri so all structural checks use the same tokeniser
        // that built the baseline — no raw-string heuristics.
        let uri = rendered
            .parse::<Uri>()
            .map_err(|_| ConfineError::UriParseFailed {
                rendered: rendered.to_string(),
            })?;

        // 2. Authority check: scheme + host must exactly match the base.
        //    Catches @-userinfo injection and host-extension attacks.
        //    Scheme/authority are normalized to lowercase; path-prefix is case-sensitive.
        let actual_scheme = uri.scheme_str().unwrap_or("").to_ascii_lowercase();
        let actual_authority = uri
            .authority()
            .map(|a| a.as_str().to_ascii_lowercase())
            .unwrap_or_default();
        if actual_scheme != self.scheme || actual_authority != self.authority {
            return Err(ConfineError::UriAuthorityMismatch {
                rendered: rendered.to_string(),
                expected: format!("{}://{}", self.scheme, self.authority),
                actual: format!("{actual_scheme}://{actual_authority}"),
            });
        }

        // 3. Path-prefix check: catches path escape when the template includes
        //    a static path (e.g. `https://api.internal/ingest/{{ tenant }}`).
        let path = uri.path();
        if !path.starts_with(&self.path_prefix) {
            return Err(ConfineError::OutsideBase {
                rendered: rendered.to_string(),
                base: format!("{}://{}{}", self.scheme, self.authority, self.path_prefix),
            });
        }

        // 4. Dot-dot segment check: catches within-prefix path traversal.
        //    Also rejects percent-encoded variants that some servers decode
        //    before resolving the path (e.g. `/ingest/%2e%2e/admin`).
        for segment in path.split('/') {
            if segment == ".."
                || segment.eq_ignore_ascii_case("%2e%2e")
                || segment.eq_ignore_ascii_case(".%2e")
                || segment.eq_ignore_ascii_case("%2e.")
            {
                return Err(ConfineError::DotDotSegment {
                    rendered: rendered.to_string(),
                });
            }
        }

        // 5. Reject encoded path separators, raw backslashes, and encoded
        //    percent signs.
        //    `%2f` (encoded `/`) and `%5c` (encoded `\`) are decoded by many
        //    servers before path normalization, turning an otherwise-safe
        //    segment into a traversal vector.  Raw `\` is accepted by
        //    `http::Uri` but treated as a path separator by Windows/IIS,
        //    allowing `/ingest/..\admin` to escape the prefix on those hosts.
        //    `%25` is the encoded form of `%`; a proxy that decodes once
        //    turns `%252e%252e%252fadmin` into `%2e%2e%2fadmin`, which a
        //    second decoder resolves to `../admin`. Rejecting `%25` closes
        //    the double-encoding bypass.
        let path_lc = path.to_ascii_lowercase();
        if path_lc.contains("%2f")
            || path_lc.contains("%5c")
            || path_lc.contains("%25")
            || path.contains('\\')
        {
            return Err(ConfineError::DotDotSegment {
                rendered: rendered.to_string(),
            });
        }

        // 6. Reject any rendered query. URI templates containing `?` are
        //    rejected at build time, so a query at render time means a field
        //    value smuggled `?...` into the path — e.g. tenant value
        //    `ok?tenant=evil` renders `.../ingest/ok?tenant=evil`: same
        //    authority and path prefix but an attacker-controlled query.
        if uri.query().is_some() {
            return Err(ConfineError::OutsideBase {
                rendered: rendered.to_string(),
                base: format!("{}://{}{}", self.scheme, self.authority, self.path_prefix),
            });
        }

        Ok(())
    }
}

impl ConfinementConfig {
    /// Returns a `ConfinementConfig` that opts out of confinement.
    ///
    /// Use only in tests where templates intentionally have no literal prefix.
    pub const fn unconfined() -> Self {
        Self {
            dangerously_allow_unconfined_template_resolution: true,
        }
    }

    /// Logs a per-template SECURITY warning on the opt-out path.
    ///
    /// The `vector_security_confinement_disabled` gauge is owned by the topology
    /// (see `RunningTopology::refresh_confinement_gauges`), which holds a handle
    /// for the sink's lifetime so the metric matches the active topology and
    /// never expires while the sink runs.
    pub fn warn_unconfined_template(
        component_kind: &'static str,
        component_type: &'static str,
        field: &'static str,
    ) {
        warn!(
            message = "SECURITY: component has `dangerously_allow_unconfined_template_resolution` \
                       enabled — template is NOT confined. A log producer that controls any \
                       field used in the template can write to arbitrary keys.",
            component_kind, component_type, field,
        );
    }
}
use super::*;
