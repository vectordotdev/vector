//! Graduated deprecation policy for configuration fields.
//!
//! When a field is removed upstream, dropping it cold breaks every config that
//! still mentions it — even though the value is no longer doing anything. This
//! module lets a removed field stay accepted (and ignored) for a window of
//! minor releases, with a `warn!` at startup that names the *exact Vector
//! version* in which the field will become a hard config-load error. Operators
//! get a clear deadline; nobody is surprised by a silent breaking change.
//!
//! The policy is per-field: declare it once at the call site by calling
//! [`check_deprecated_field`] with the field's path and the minor version in
//! which it was removed. Reuse the same function for any future deprecations.
//!
//! # Versioning model
//!
//! Vector versions are `<major>.<minor>.<patch>`. This module compares only
//! `(major, minor)` (parsed from `CARGO_PKG_VERSION` at compile time).
//!
//! Given a field deprecated in minor `D`:
//!
//! * Current minor in `D..=D + WARN_WINDOW_MINORS` (≈one year of releases) →
//!   log a `warn!` and treat the field as ignored. Caller proceeds normally.
//! * Current minor `> D + WARN_WINDOW_MINORS` → return an `Err` with a clear
//!   "remove this field" message. Callers should propagate to a config-load
//!   failure.
//!
//! The warning text always names `breaks_in = D + WARN_WINDOW_MINORS + 1` so
//! operators know exactly which release will reject their config.

/// Number of minor releases the field is accepted-with-warning after the
/// version in which it was removed (inclusive of the removal minor itself).
///
/// At Vector's roughly-monthly minor cadence, 12 corresponds to about a year
/// of advance notice before the field becomes a hard configuration error.
pub const WARN_WINDOW_MINORS: u64 = 12;

/// A `(major, minor)` Vector version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VectorMinor {
    pub major: u64,
    pub minor: u64,
}

impl VectorMinor {
    pub const fn new(major: u64, minor: u64) -> Self {
        Self { major, minor }
    }

    /// First minor at which a field deprecated in `self` becomes a hard error.
    pub const fn breaks_in(self) -> Self {
        Self {
            major: self.major,
            minor: self.minor + WARN_WINDOW_MINORS + 1,
        }
    }
}

impl std::fmt::Display for VectorMinor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Patch is always 0 for the version reference shown to users.
        write!(f, "{}.{}.0", self.major, self.minor)
    }
}

/// Stage of deprecation a field is in given the current Vector version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeprecationStage {
    /// In the warn window. The field is accepted at deserialize time but
    /// has no effect at runtime; a `warn!` should fire at startup.
    Warn { breaks_in: VectorMinor },
    /// Past the warn window. The field must be rejected.
    Error,
}

/// Compute the deprecation stage for a field that was removed in `deprecated_in`,
/// given the running Vector binary's `(major, minor)`.
///
/// Pure function; takes `current` so it can be unit-tested without depending on
/// the build's actual version.
pub fn stage_for(deprecated_in: VectorMinor, current: VectorMinor) -> DeprecationStage {
    let breaks_in = deprecated_in.breaks_in();
    let past_break = (current.major, current.minor) >= (breaks_in.major, breaks_in.minor);
    if past_break {
        DeprecationStage::Error
    } else {
        DeprecationStage::Warn { breaks_in }
    }
}

/// `(major, minor)` of the running binary, parsed from `CARGO_PKG_VERSION`.
pub fn current_minor() -> VectorMinor {
    let v = env!("CARGO_PKG_VERSION");
    let mut parts = v.split('.');
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    // CARGO_PKG_VERSION can carry a pre-release suffix on `minor` (e.g. "55-rc1");
    // strip anything after the first non-digit so we still parse cleanly.
    let minor_raw = parts.next().unwrap_or("0");
    let minor_str: String = minor_raw
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let minor = minor_str.parse().unwrap_or(0);
    VectorMinor { major, minor }
}

/// Check a deprecated field that the user has set, using the running binary's
/// version.
///
/// * `field_path` — user-facing dotted path, e.g. `"api.playground"`.
/// * `deprecated_in` — minor version in which the field stopped having any
///   effect.
/// * `note` — short, field-specific context appended to the message
///   (e.g. *"The GraphQL Playground was removed when the API moved to gRPC."*).
///
/// Returns `Ok(())` after emitting a `warn!` while in the warn window;
/// returns `Err` with a config-load–ready message once past the window.
pub fn check_deprecated_field(
    field_path: &'static str,
    deprecated_in: VectorMinor,
    note: &str,
) -> Result<(), String> {
    check_deprecated_field_at(field_path, deprecated_in, note, current_minor())
}

/// Same as [`check_deprecated_field`] but takes an explicit current version,
/// for tests.
pub fn check_deprecated_field_at(
    field_path: &'static str,
    deprecated_in: VectorMinor,
    note: &str,
    current: VectorMinor,
) -> Result<(), String> {
    match stage_for(deprecated_in, current) {
        DeprecationStage::Warn { breaks_in } => {
            warn!(
                message = format!(
                    "`{field_path}` is deprecated and ignored. {note} \
                     It will be rejected as a configuration error in Vector {breaks_in}. \
                     Remove `{field_path}` from your configuration to silence this warning."
                ),
                internal_log_rate_limit = false,
            );
            Ok(())
        }
        DeprecationStage::Error => Err(format!(
            "`{field_path}` was removed and is no longer accepted. {note} \
             Remove `{field_path}` from your configuration."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REMOVED_IN: VectorMinor = VectorMinor::new(0, 55);

    #[test]
    fn breaks_in_is_window_plus_one() {
        assert_eq!(
            REMOVED_IN.breaks_in(),
            VectorMinor::new(0, 55 + WARN_WINDOW_MINORS + 1)
        );
    }

    #[test]
    fn warn_at_removal_minor() {
        assert!(matches!(
            stage_for(REMOVED_IN, VectorMinor::new(0, 55)),
            DeprecationStage::Warn { .. }
        ));
    }

    #[test]
    fn warn_throughout_window() {
        for delta in 0..=WARN_WINDOW_MINORS {
            let current = VectorMinor::new(0, 55 + delta);
            assert!(
                matches!(
                    stage_for(REMOVED_IN, current),
                    DeprecationStage::Warn { .. }
                ),
                "expected Warn at {current}"
            );
        }
    }

    #[test]
    fn error_at_first_post_window_minor() {
        assert_eq!(
            stage_for(
                REMOVED_IN,
                VectorMinor::new(0, 55 + WARN_WINDOW_MINORS + 1)
            ),
            DeprecationStage::Error
        );
    }

    #[test]
    fn error_far_past_window() {
        assert_eq!(
            stage_for(REMOVED_IN, VectorMinor::new(1, 0)),
            DeprecationStage::Error
        );
    }

    #[test]
    fn check_returns_ok_with_warning_in_window() {
        let r = check_deprecated_field_at(
            "api.playground",
            REMOVED_IN,
            "GraphQL Playground was removed.",
            VectorMinor::new(0, 60),
        );
        assert!(r.is_ok());
    }

    #[test]
    fn check_returns_err_past_window() {
        let r = check_deprecated_field_at(
            "api.playground",
            REMOVED_IN,
            "GraphQL Playground was removed.",
            VectorMinor::new(0, 55 + WARN_WINDOW_MINORS + 1),
        );
        let msg = r.expect_err("must err past the window");
        assert!(msg.contains("api.playground"));
        assert!(msg.contains("removed"));
    }

    #[test]
    fn warn_message_names_breaks_in_version() {
        // Confirm the breaks_in version reaches Display formatting cleanly.
        // (The macro emits to tracing; we assert formatting on VectorMinor itself.)
        assert_eq!(
            format!("{}", REMOVED_IN.breaks_in()),
            format!("0.{}.0", 55 + WARN_WINDOW_MINORS + 1)
        );
    }

    #[test]
    fn current_minor_parses_pkg_version() {
        // We can't assert specific numbers without coupling to the version,
        // but we can at least confirm parsing returns nonzero for a real build.
        let v = current_minor();
        assert!(
            v.major > 0 || v.minor > 0,
            "CARGO_PKG_VERSION should yield non-zero major or minor"
        );
    }
}
