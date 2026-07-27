#[derive(Debug, Snafu)]
#[snafu(module(build_error))]
pub(crate) enum BuildError {
    /// Template has event-field references but no literal prefix to confine them to.
    #[snafu(display(
        "template references event fields ({fields:?}) but has no \
         literal string prefix to derive a confinement base from. Add a static \
         prefix to your template, or set \
         `dangerously_allow_unconfined_template_resolution: true` to opt out."
    ))]
    NoDerivableBase {
        /// The event fields referenced by the template.
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
         the authority (host) component: the static prefix does not contain a \
         `/` after the host, so the rendered host is partly event-controlled. \
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
/// Dispatches to `PrefixChecker` for non-URI fields (Kafka topics, Redis
/// keys, tenant IDs, …) or `UriChecker` for HTTP/HTTPS URI fields. Common
/// guards (NUL bytes, length limit) run before dispatching.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ConfinementChecker {
    Prefix(PrefixChecker),
    Uri(UriChecker),
}

impl ConfinementChecker {
    pub(crate) fn for_template(tpl: &Template) -> Result<Option<Self>, BuildError> {
        let fields = match tpl.get_fields() {
            Some(f) => f,
            None => return Ok(None),
        };
        let prefix = tpl.literal_prefix();
        if prefix.is_empty() {
            return Err(BuildError::NoDerivableBase { fields });
        }
        if prefix == "/" {
            return Err(BuildError::DerivedBaseIsRoot {
                prefix: prefix.to_string(),
            });
        }
        // Only treat as a URI if the prefix literally starts with http:// or https://.
        // Testing for the scheme token alone (without "://") would misfire on
        // templates like `http_{{tenant}}` whose prefix is `http_`.
        let lp = prefix.to_ascii_lowercase();
        if lp.starts_with("http://") || lp.starts_with("https://") {
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
            UriChecker::from_prefix(prefix).map(|c| Some(Self::Uri(c)))
        } else {
            Ok(Some(Self::Prefix(PrefixChecker {
                base: prefix.to_string(),
            })))
        }
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
/// `http::Uri` and its scheme, authority, and path are stored normalised
/// (lowercased). At render time the rendered value is also parsed with
/// `http::Uri` and the structured fields are compared, which avoids all the
/// pitfalls of raw-string heuristics (case sensitivity, percent-encoding,
/// `@`-injection inside the authority component, etc.).
///
/// Three checks run on every render:
///
/// 1. **Authority check** — the rendered URI's scheme and authority must match
///    the operator-authored values exactly. This catches `@`-userinfo injection
///    (`trusted.host@evil.com`) and host-extension attacks
///    (`trusted.host.evil.com`).
///
/// 2. **Path-prefix check** — the rendered URI's path must start with the
///    static path portion derived from the template prefix.
///
/// 3. **Dot-dot segment check** — no path segment may be `..`, `.%2e`,
///    `%2e.`, or `%2e%2e` (case-insensitive). This catches path traversal
///    within the same host even when the prefix check passes.
///
/// URI templates containing `?` are rejected at build time — query parameters
/// cannot be safely confined to a static boundary and must not appear in
/// dynamic URI templates.
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
    pub(crate) fn from_prefix(prefix: &str) -> Result<Self, BuildError> {
        let uri = prefix
            .parse::<Uri>()
            .map_err(|_| BuildError::NoStaticUriAuthority {
                prefix: prefix.to_string(),
            })?;
        let scheme = uri
            .scheme_str()
            .expect("scheme present because prefix starts with http(s)://")
            .to_ascii_lowercase();
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
        let authority = match uri.authority() {
            Some(auth) if !auth.as_str().is_empty() => auth.as_str().to_ascii_lowercase(),
            _ => {
                return Err(BuildError::NoStaticUriAuthority {
                    prefix: prefix.to_string(),
                });
            }
        };
        // `http::Uri` normalises a missing path to `"/"`, so `uri.path()` can't
        // tell us whether the prefix actually had a `/` closing off the host. Check
        // the raw prefix instead: no `/` after `://` means the `{{ field }}`
        // reference sits inside (or extends) the authority we just parsed.
        let after_scheme = prefix
            .split_once("://")
            .map(|(_, rest)| rest)
            .expect("\"://\" present because prefix starts with http(s)://");
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
        // Parse with http::Uri so all structural checks use the same tokeniser
        // that built the baseline — no raw-string heuristics.
        let uri = rendered
            .parse::<Uri>()
            .map_err(|_| ConfineError::UriParseFailed {
                rendered: rendered.to_string(),
            })?;

        // 1. Authority check: scheme + host must exactly match the base.
        //    Catches @-userinfo injection and host-extension attacks.
        //    Both sides are lowercased so the comparison is case-insensitive.
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

        // 2. Path-prefix check: catches path escape when the template includes
        //    a static path (e.g. `https://api.internal/ingest/{{ tenant }}`).
        let path = uri.path();
        if !path.starts_with(&self.path_prefix) {
            return Err(ConfineError::OutsideBase {
                rendered: rendered.to_string(),
                base: format!("{}://{}{}", self.scheme, self.authority, self.path_prefix),
            });
        }

        // 3. Dot-dot segment check: catches within-prefix path traversal.
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

        // 4. Reject encoded path separators, raw backslashes, and encoded
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

        // 4. Reject any rendered query. URI templates containing `?` are
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

/// Serializable config fragment for template confinement.
///
/// Embed this in a component config with `#[serde(flatten)]` to get the
/// `dangerously_allow_unconfined_template_resolution` field. Pass it to
/// [`Template::confine`] on each template the component owns.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct ConfinementConfig {
    /// Disable all template confinement checks for this sink.
    ///
    /// **DANGEROUS — disables a security control.**
    ///
    /// Bypasses both startup validation and runtime confinement for every
    /// templated field on this sink. When enabled, a log producer that
    /// controls any field used in a template can write to arbitrary keys,
    /// paths, or routing destinations. This flag is a full opt-out: it
    /// disables confinement even for templates that have a usable static
    /// prefix.
    #[serde(default)]
    pub dangerously_allow_unconfined_template_resolution: bool,
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
