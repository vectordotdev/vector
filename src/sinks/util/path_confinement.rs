//! Shared infrastructure for confining templated sink outputs to an
//! operator-authored boundary.
//!
//! Sinks that render templates into security-relevant identifiers (paths,
//! keys, URIs, …) use the helpers in this module to ensure the rendered
//! value cannot escape the literal portion the operator wrote.

use std::path::{Component, Path, PathBuf};

use snafu::Snafu;
use tokio::fs as tokio_fs;

use crate::template::Template;

/// Maximum byte length of a rendered path before it is rejected.
///
/// Bounds per-event cost (path canonicalization, directory creation) and
/// provides a coarse cap on memory blow-up from attacker-controlled fields.
pub const MAX_RENDERED_PATH_LEN: usize = 1024;

/// Errors raised while building a [`PathConfinement`] from a template.
#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display(
        "path template references event fields ({fields:?}) but has no \
         literal directory prefix to derive a base directory from. Set \
         `base_dir` explicitly, or set \
         `dangerously_allow_unconfined_template_resolution: true` to opt out of path \
         confinement (not recommended)."
    ))]
    NoDerivableBase { fields: Vec<String> },

    #[snafu(display(
        "path template literal prefix {prefix:?} normalizes to a filesystem \
         root, which would permit writes anywhere on disk. Set `base_dir` \
         explicitly (for example `base_dir: /var/log/vector`), or set \
         `dangerously_allow_unconfined_template_resolution: true` to opt out of path \
         confinement (not recommended)."
    ))]
    DerivedBaseIsRoot { prefix: String },

    #[snafu(display("`base_dir` must be an absolute path, got {path:?}"))]
    BaseNotAbsolute { path: PathBuf },
}

/// Errors raised while confining a rendered path against a base directory.
#[derive(Debug, Snafu)]
pub enum ConfineError {
    #[snafu(display("rendered path contains a NUL byte"))]
    NulByte,

    #[snafu(display(
        "rendered path {rendered:?} resolves outside the configured base \
         directory {base:?}"
    ))]
    OutsideBase { rendered: PathBuf, base: PathBuf },

    #[snafu(display("rendered path is {len} bytes; maximum allowed is {max}"))]
    TooLong { len: usize, max: usize },

    #[cfg(windows)]
    #[snafu(display("rendered path contains a forbidden Windows component: {component:?}"))]
    ForbiddenComponent { component: String },

    #[snafu(display(
        "rendered path {parent:?} resolves outside the base directory \
         {base:?} after symlink resolution"
    ))]
    SymlinkEscape { parent: PathBuf, base: PathBuf },

    #[snafu(display("I/O error while resolving base directory {path:?}: {source}"))]
    BaseIo {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Lexically resolve `.` and `..` in a path without touching the
/// filesystem.
///
/// This is pure: it never follows symlinks, never reads the FS, and never
/// pops past a root or prefix component. The result has the same root /
/// prefix as the input.
pub fn normalize_lexically(p: &Path) -> PathBuf {
    let mut out: Vec<Component<'_>> = Vec::new();
    for component in p.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                out.push(component);
            }
            Component::CurDir => {}
            Component::ParentDir => {
                // Only pop if the last pushed component is a normal segment.
                // Never pop past a root or prefix. If there is no normal
                // segment to pop and no root anchor, retain the `..`
                // (relative path).
                let pop_idx = out.iter().rposition(|c| matches!(c, Component::Normal(_)));
                match pop_idx {
                    Some(idx) if idx == out.len() - 1 => {
                        out.pop();
                    }
                    _ => {
                        // No trailing Normal to pop.
                        let has_anchor = out
                            .iter()
                            .any(|c| matches!(c, Component::Prefix(_) | Component::RootDir));
                        if !has_anchor {
                            out.push(component);
                        }
                    }
                }
            }
            Component::Normal(_) => {
                out.push(component);
            }
        }
    }
    let mut buf = PathBuf::new();
    for c in out {
        buf.push(c.as_os_str());
    }
    if buf.as_os_str().is_empty() {
        buf.push(".");
    }
    buf
}

/// Returns `true` if `p` is exactly a filesystem root (no normal segments
/// below the root or drive prefix).
fn is_filesystem_root(p: &Path) -> bool {
    let mut had_anchor = false;
    for c in p.components() {
        match c {
            Component::Prefix(_) | Component::RootDir => had_anchor = true,
            Component::CurDir => {}
            _ => return false,
        }
    }
    had_anchor
}

/// Truncate a literal-prefix string to the last path-separator boundary so
/// that the returned slice is a clean directory prefix (no trailing partial
/// component like `srv-` in `"/srv-{{id}}"`).
fn truncate_to_separator(prefix: &str) -> &str {
    let bytes = prefix.as_bytes();
    let mut cut = 0usize;
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'/' || (cfg!(windows) && *b == b'\\') {
            cut = i + 1;
        }
    }
    prefix.split_at(cut).0
}

/// Confines a rendered filesystem path to a base directory derived from a
/// template's literal prefix.
///
/// Build with [`PathConfinement::for_template`] at sink construction time
/// (no FS I/O). Use [`PathConfinement::confine`] before any FS mutation,
/// and [`PathConfinement::verify_parent`] after `create_dir_all` to catch
/// intermediate symlinks.
#[derive(Debug)]
pub struct PathConfinement {
    base_lexical: PathBuf,
    base_canonical: Option<PathBuf>,
}

impl PathConfinement {
    /// Build a confinement for `tpl`. Returns:
    /// - `Ok(None)` if the template has no field references (nothing to confine).
    /// - `Ok(Some(_))` with a base derived from `explicit` (if set) or from
    ///   the template's literal prefix.
    /// - `Err(_)` if no usable base can be derived and `explicit` is unset.
    ///
    /// Performs no filesystem I/O.
    pub fn for_template(
        tpl: &Template,
        explicit: Option<&Path>,
    ) -> Result<Option<Self>, BuildError> {
        let fields = match tpl.get_fields() {
            Some(f) => f,
            None => return Ok(None),
        };

        let base_path = match explicit {
            Some(p) => {
                if !p.is_absolute() {
                    return Err(BuildError::BaseNotAbsolute {
                        path: p.to_path_buf(),
                    });
                }
                normalize_lexically(p)
            }
            None => {
                let raw = tpl.literal_prefix();
                let dir_prefix = truncate_to_separator(raw);
                if dir_prefix.is_empty() {
                    return Err(BuildError::NoDerivableBase { fields });
                }
                let candidate = normalize_lexically(Path::new(dir_prefix));
                if !candidate.is_absolute() {
                    return Err(BuildError::NoDerivableBase { fields });
                }
                if is_filesystem_root(&candidate) {
                    return Err(BuildError::DerivedBaseIsRoot {
                        prefix: dir_prefix.to_owned(),
                    });
                }
                candidate
            }
        };

        if explicit.is_some() && is_filesystem_root(&base_path) {
            warn!(
                message = "Configured `base_dir` is a filesystem root; path \
                           confinement is effectively disabled.",
                base_dir = ?base_path,
            );
        }

        Ok(Some(Self {
            base_lexical: base_path,
            base_canonical: None,
        }))
    }

    /// The lexical base directory used for containment checks.
    pub fn base_dir(&self) -> &Path {
        &self.base_lexical
    }

    /// Apply lexical confinement to a rendered path. Pure — runs before
    /// any FS mutation.
    pub fn confine(&self, rendered: &Path) -> Result<PathBuf, ConfineError> {
        let raw_bytes = path_bytes(rendered);
        if raw_bytes.contains(&0) {
            return Err(ConfineError::NulByte);
        }
        if raw_bytes.len() > MAX_RENDERED_PATH_LEN {
            return Err(ConfineError::TooLong {
                len: raw_bytes.len(),
                max: MAX_RENDERED_PATH_LEN,
            });
        }

        let absolute = if rendered.is_absolute() {
            rendered.to_path_buf()
        } else {
            self.base_lexical.join(rendered)
        };
        let normalized = normalize_lexically(&absolute);

        #[cfg(windows)]
        {
            for c in normalized.components() {
                if let Component::Normal(os) = c {
                    let s = os.to_string_lossy();
                    if s.contains(':') {
                        return Err(ConfineError::ForbiddenComponent {
                            component: s.into_owned(),
                        });
                    }
                    if is_windows_reserved_name(&s) {
                        return Err(ConfineError::ForbiddenComponent {
                            component: s.into_owned(),
                        });
                    }
                }
            }
        }

        if !normalized.starts_with(&self.base_lexical) {
            return Err(ConfineError::OutsideBase {
                rendered: normalized,
                base: self.base_lexical.clone(),
            });
        }

        Ok(normalized)
    }

    /// Verify that `parent` (typically the parent directory of the file
    /// about to be opened) canonicalizes to a location inside the confined
    /// base directory.
    ///
    /// Catches intermediate symlinks placed by an **event-field attacker**
    /// (a log producer that controls field values but cannot write to the
    /// filesystem). A local attacker who can write inside `base_dir` could
    /// race between this call and the subsequent `open` to swap a directory
    /// for a symlink; closing that gap requires fd-based traversal
    /// (`openat`/`cap-std`), which is Phase 1b scope.
    pub async fn verify_parent(&mut self, parent: &Path) -> Result<PathBuf, ConfineError> {
        if self.base_canonical.is_none() {
            tokio_fs::create_dir_all(&self.base_lexical)
                .await
                .map_err(|source| ConfineError::BaseIo {
                    path: self.base_lexical.clone(),
                    source,
                })?;
            let canonical = tokio_fs::canonicalize(&self.base_lexical)
                .await
                .map_err(|source| ConfineError::BaseIo {
                    path: self.base_lexical.clone(),
                    source,
                })?;
            self.base_canonical = Some(canonical);
        }
        let base_canonical = self.base_canonical.as_ref().expect("just set");

        let parent_canonical =
            tokio_fs::canonicalize(parent)
                .await
                .map_err(|source| ConfineError::BaseIo {
                    path: parent.to_path_buf(),
                    source,
                })?;

        if !parent_canonical.starts_with(base_canonical) {
            return Err(ConfineError::SymlinkEscape {
                parent: parent_canonical,
                base: base_canonical.clone(),
            });
        }

        Ok(parent_canonical)
    }
}

#[cfg(unix)]
fn path_bytes(p: &Path) -> &[u8] {
    use std::os::unix::ffi::OsStrExt;
    p.as_os_str().as_bytes()
}

#[cfg(not(unix))]
fn path_bytes(p: &Path) -> &[u8] {
    // On Windows OsStr is WTF-8; falling back to to_string_lossy is fine
    // for length and NUL-byte checks because a real NUL survives lossy
    // conversion.
    p.as_os_str().to_str().map(str::as_bytes).unwrap_or(&[])
}

#[cfg(windows)]
fn is_windows_reserved_name(name: &str) -> bool {
    // Strip extension for the check.
    let stem = name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(name)
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.starts_with("COM")
            && stem.len() == 4
            && matches!(stem.as_bytes()[3], b'0'..=b'9' | 0xB9 | 0xB2 | 0xB3))
        || (stem.starts_with("LPT")
            && stem.len() == 4
            && matches!(stem.as_bytes()[3], b'0'..=b'9' | 0xB9 | 0xB2 | 0xB3))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pb(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn normalize_lexically_cases() {
        let cases: &[(&str, &str)] = &[
            ("/a/b/../c", "/a/c"),
            ("/a/./b", "/a/b"),
            ("/a//b", "/a/b"),
            ("/..", "/"),
            ("/../../etc", "/etc"),
            ("../a", "../a"),
            ("a/../../b", "../b"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                normalize_lexically(&pb(input)),
                pb(expected),
                "input = {input:?}"
            );
        }
    }

    // Path-shape cases hard-code Unix absolute paths (`/var/log/...`) so the
    // absolute/root/derived-base semantics apply. On Windows `/foo` is not
    // absolute and would surface `NoDerivableBase` for cases that expect
    // `DerivedBaseIsRoot`. The confinement logic itself is platform-neutral;
    // only the string fixtures are Unix-shaped.
    #[cfg(unix)]
    #[test]
    fn for_template_cases() {
        enum Expected {
            Static,
            Base(&'static str),
            ErrNoDerivable,
            ErrDerivedIsRoot,
            ErrNotAbsolute,
        }
        use Expected::*;
        let cases: &[(&str, Option<&str>, Expected)] = &[
            // no field refs → no confinement needed
            ("/var/log/app.log", None, Static),
            // auto-derives base from literal prefix
            ("/var/log/{{ host }}/app.log", None, Base("/var/log")),
            // partial component before field → truncates to root → rejected
            ("/srv-{{ id }}.log", None, ErrDerivedIsRoot),
            // no literal prefix at all
            ("{{ full_path }}", None, ErrNoDerivable),
            // only `/` before first field
            ("/{{ tenant }}/app.log", None, ErrDerivedIsRoot),
            // explicit base overrides auto-derived
            (
                "/var/log/{{ host }}/app.log",
                Some("/srv/tenants"),
                Base("/srv/tenants"),
            ),
            // `%%` stops scan conservatively; base truncates to /tmp
            ("/tmp/100%%/{{ x }}.log", None, Base("/tmp")),
            // relative explicit base is always rejected
            ("{{ x }}", Some("relative/dir"), ErrNotAbsolute),
        ];
        for (tpl_src, explicit, expected) in cases {
            let tpl = Template::try_from(*tpl_src).unwrap();
            let explicit_path = explicit.map(Path::new);
            let result = PathConfinement::for_template(&tpl, explicit_path);
            match expected {
                Static => assert!(result.unwrap().is_none(), "expected None for {tpl_src:?}"),
                Base(b) => assert_eq!(
                    result.unwrap().unwrap().base_dir(),
                    pb(b),
                    "tpl = {tpl_src:?}"
                ),
                ErrNoDerivable => assert!(
                    matches!(result.unwrap_err(), BuildError::NoDerivableBase { .. }),
                    "tpl = {tpl_src:?}"
                ),
                ErrDerivedIsRoot => assert!(
                    matches!(result.unwrap_err(), BuildError::DerivedBaseIsRoot { .. }),
                    "tpl = {tpl_src:?}"
                ),
                ErrNotAbsolute => assert!(
                    matches!(result.unwrap_err(), BuildError::BaseNotAbsolute { .. }),
                    "tpl = {tpl_src:?}"
                ),
            }
        }
    }

    // Same reasoning as `for_template_cases` — Unix-shaped fixtures.
    #[cfg(unix)]
    #[test]
    fn confine_cases() {
        // (template, rendered_path, expect_ok)
        let cases: &[(&str, &str, bool)] = &[
            // legitimate sub-path
            (
                "/var/log/{{ host }}/app.log",
                "/var/log/host-a/app.log",
                true,
            ),
            // `..` escape
            (
                "/var/log/apps/{{ s }}/app.log",
                "/var/log/apps/../../../etc/cron.d/app.log",
                false,
            ),
            // absolute path outside base
            ("/var/log/{{ host }}/app.log", "/etc/passwd", false),
            // string-prefix confusion: /var/logs ≠ /var/log
            ("/var/log/{{ x }}", "/var/logs/x", false),
            // %% base is /tmp; both %% and % variants fall under it
            ("/tmp/100%%/{{ x }}.log", "/tmp/100%%/value.log", true),
            ("/tmp/100%%/{{ x }}.log", "/tmp/100%/value.log", true),
        ];
        for (tpl_src, rendered, expect_ok) in cases {
            let tpl = Template::try_from(*tpl_src).unwrap();
            let c = PathConfinement::for_template(&tpl, None).unwrap().unwrap();
            let result = c.confine(Path::new(rendered));
            assert_eq!(
                result.is_ok(),
                *expect_ok,
                "tpl={tpl_src:?} rendered={rendered:?} → {result:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn confine_blocks_nul_byte() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let tpl = Template::try_from("/var/log/{{ x }}").unwrap();
        let c = PathConfinement::for_template(&tpl, None).unwrap().unwrap();
        let p = Path::new(OsStr::from_bytes(b"/var/log/abc\0def"));
        assert!(matches!(c.confine(p).unwrap_err(), ConfineError::NulByte));
    }
}
