//! [`Glob`] based paths provider implementation.

use std::path::{Path, PathBuf};

use file_source_common::internal_events::FileSourceInternalEvents;
pub use glob::MatchOptions;
use glob::Pattern;

/// Represents the ability to enumerate paths.
///
/// For use in [`crate::file_server::FileServer`].
///
/// # Notes
///
/// Ideally we'd use an iterator with bound lifetime here:
///
/// ```ignore
/// type Iter<'a>: Iterator<Item = PathBuf> + 'a;
/// fn paths(&self) -> Self::Iter<'_>;
/// ```
///
/// However, that's currently unavailable at Rust.
/// See: <https://github.com/rust-lang/rust/issues/44265>
///
/// We use an `IntoIter` here as a workaround.
pub trait PathsProvider {
    /// Provides the iterator that returns paths.
    type IntoIter: IntoIterator<Item = PathBuf>;

    /// Provides a set of paths.
    fn paths(&self) -> Self::IntoIter;

    /// Provides the raw patterns (or literal roots) this provider globs/scans from, for use by
    /// event-driven discovery (see [`crate::notify_watcher`]) to compute which directories to
    /// watch for OS-level filesystem notifications.
    ///
    /// Defaults to an empty vec, meaning "no known roots" -- implementors that don't override
    /// this simply won't participate in notify-based discovery (`FileServer` will fall back to
    /// polling-only behavior for such providers, since it has nothing to watch).
    fn watch_roots(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    /// Whether `path` is one this provider would yield, decided without enumerating anything.
    ///
    /// Event-driven discovery needs this: notify watches whole *directories*, so an event can name
    /// any file under them, including ones the rules leave out. Answering with [`Self::paths`]
    /// would walk the entire include tree on every event, which is exactly the O(N)-per-event cost
    /// event-driven discovery exists to avoid (see [`crate::file_server`]).
    ///
    /// `None` means "cannot decide cheaply" -- the default, so a provider that doesn't override
    /// this keeps its previous behavior and its caller falls back to a full reconciliation pass.
    fn is_included(&self, _path: &Path) -> Option<bool> {
        None
    }
}

/// A glob-based path provider.
///
/// Provides the paths to the files on the file system that match include
/// patterns and don't match the exclude patterns.
pub struct Glob<E: FileSourceInternalEvents> {
    include_patterns: Vec<String>,
    exclude_patterns: Vec<Pattern>,
    glob_match_options: MatchOptions,
    emitter: E,
}

impl<E: FileSourceInternalEvents> Glob<E> {
    /// Create a new [`Glob`].
    ///
    /// Returns `None` if patterns aren't valid.
    pub fn new(
        include_patterns: &[PathBuf],
        exclude_patterns: &[PathBuf],
        glob_match_options: MatchOptions,
        emitter: E,
    ) -> Option<Self> {
        let include_patterns = include_patterns
            .iter()
            .map(|path| path.to_str().map(ToOwned::to_owned))
            .collect::<Option<_>>()?;

        let exclude_patterns = exclude_patterns
            .iter()
            .filter_map(|path| path.to_str().map(|path| Pattern::new(path).ok()))
            .collect::<Option<Vec<_>>>()?;

        Some(Self {
            include_patterns,
            exclude_patterns,
            glob_match_options,
            emitter,
        })
    }
}

/// The spelling a glob over `include_pattern` would have emitted for `absolute_path`.
///
/// `paths()` compares its candidates against the raw exclusion patterns, and those candidates carry
/// the pattern's own spelling: `glob` keeps a relative pattern relative, an absolute one absolute,
/// and leaves `.`/`..` components as written. Exclusions must therefore be matched against the same
/// spelling, which is the pattern's literal prefix with the remainder of the event path appended.
///
/// `None` when the two cannot be reconciled, leaving the caller to fall back to the absolute form.
fn candidate_spelling(
    include_pattern: &str,
    absolute_pattern: &Path,
    absolute_path: &Path,
) -> Option<PathBuf> {
    // The pattern's leading components up to the first glob metacharacter are literal, and the
    // normalized pattern shares that prefix with the normalized event path.
    let literal_prefix: PathBuf = absolute_pattern
        .components()
        .take_while(|component| {
            component
                .as_os_str()
                .to_str()
                .is_some_and(|component| !component.contains(['*', '?', '[', '{']))
        })
        .collect();
    let remainder = absolute_path.strip_prefix(&literal_prefix).ok()?;

    // Rebuild using the pattern's own literal prefix, with `.` components dropped and `..` kept --
    // which is exactly what `glob` does to the candidates it emits (both measured): `./logs/*.log`
    // yields `logs/app.log`, while `logs/../logs/*.log` yields `logs/../logs/app.log`. Resolving
    // `..` here as well made a `..`-spelled exclusion miss its own file.
    let written_prefix: PathBuf = Path::new(include_pattern)
        .components()
        .take_while(|component| {
            component
                .as_os_str()
                .to_str()
                .is_some_and(|component| !component.contains(['*', '?', '[', '{']))
        })
        .collect();
    // A prefix that is empty, or only `.`, stays empty: `*.log` and `./*.log` both yield
    // `secret.log`, not `./secret.log` (measured), so a relative exclusion like `secret*.log` must be
    // compared against the bare name. `strip_current_dir_components` keeps a lone `.` deliberately --
    // a path's meaning depends on it -- but a glob *prefix* has no such meaning to preserve.
    let written_prefix = file_source_common::strip_current_dir_components(&written_prefix);
    let written_prefix = if written_prefix == Path::new(".") {
        PathBuf::new()
    } else {
        written_prefix
    };
    Some(written_prefix.join(remainder))
}

/// Normalize an include *pattern* for comparison against a reported path.
///
/// Like [`file_source_common::lexically_normalize`], except a `..` is only resolved when the
/// component it would cancel is a literal name. Glob expansion never folds `..` into a wildcard: for
/// `<root>/*/../*.log`, `*` expands to real directory entries and the `..` stays in the emitted
/// candidate. Measured both ways -- with a subdirectory present, glob yields
/// `<root>/sub/../x.log`; with none, glob yields *nothing*, while folding the pattern down to
/// `<root>/*.log` still matched `<root>/x.log`.
///
/// Left folded, `is_included` claimed a path the full scan never yields, so a notify event for it got
/// targeted processing and became eligible for `remove_after` cleanup. Stopping at the wildcard keeps
/// the two in agreement: the pattern retains its `..`, which then simply fails to match, and the
/// caller defers to the full pass.
fn normalize_include_pattern(pattern: &Path) -> PathBuf {
    use std::path::Component;

    let mut out = PathBuf::new();
    for component in pattern.components() {
        match component {
            // A `.` is dropped unconditionally: glob does the same, and it cancels nothing.
            Component::CurDir => {}
            Component::ParentDir => {
                let cancels_a_literal_name = out.components().next_back().is_some_and(|last| {
                    matches!(last, Component::Normal(name)
                    if name.to_str().is_some_and(|name| {
                        !crate::notify_watcher::contains_glob_metachar(name)
                    }))
                });
                if cancels_a_literal_name {
                    out.pop();
                } else {
                    // Above the root, after another `..`, or -- the case that matters -- after a
                    // wildcard, which glob expands to real entries rather than cancelling.
                    out.push(component);
                }
            }
            other => out.push(other),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// Whether `pattern` contains a `..` that string comparison cannot settle.
///
/// Three cases, each measured against real `glob` expansion:
///
/// - After a **wildcard**: glob expands it to real entries and keeps the `..`, so one file is emitted
///   as `<root>/a/../x.log` where notify reports `<root>/x.log`.
/// - After a **literal name that is not a directory**: glob cannot walk into it, so
///   `<root>/missing/../*.log` yields nothing at all -- measured both with `missing` absent and with a
///   *file* in its place -- while the folded pattern matches `<root>/x.log` happily.
/// - After a **symlink**: the kernel resolves the link before applying `..`, so the hop lands in the
///   *target's* parent, not the link's. Measured: with `/a/link` -> `/b/deep`, `/a/link/../*.log`
///   opens `/b/x.log`, while folding it lexically yields `/a/*.log`.
///
/// Only a real directory folds correctly, and must keep doing so: an include like
/// `<dir>/../<dir>/*.log` is a real configuration whose files `paths()` does yield, so `is_included`
/// owes a definite verdict there rather than deferring.
///
/// Where it cannot be settled, `is_included` answers `None`: "included" would let `remove_after`
/// delete a path outside the configured glob, "excluded" would discard an event the full scan acts on.
fn has_unresolvable_parent_dir(pattern: &Path) -> bool {
    use std::path::Component;

    let mut walked = PathBuf::new();
    for component in pattern.components() {
        match component {
            Component::ParentDir => {
                let cancels_a_wildcard = walked.components().next_back().is_some_and(|last| {
                    matches!(last, Component::Normal(name)
                    if name.to_str().is_some_and(
                        crate::notify_watcher::contains_glob_metachar))
                });
                // `symlink_metadata`, so a symlink is *not* followed: the kernel resolves the link
                // before applying `..`, so `/a/link/..` where `link` -> `/b/deep` is `/b`, while a
                // lexical pop would give `/a`. Measured: glob through such a pattern opens `/b/x.log`
                // while the folded pattern matched `/a/*.log`, letting `is_included` accept files
                // `paths()` never yields -- and `remove_after` then delete one.
                //
                // Only the directory the `..` cancels is stat'ed, and only for a `..` pattern, so an
                // ordinary include pays nothing.
                let cancels_a_real_directory = std::fs::symlink_metadata(&walked)
                    .is_ok_and(|metadata| metadata.file_type().is_dir());
                if cancels_a_wildcard || !cancels_a_real_directory {
                    return true;
                }
                walked.pop();
            }
            Component::CurDir => {}
            other => walked.push(other),
        }
    }
    false
}

impl<E: FileSourceInternalEvents> PathsProvider for Glob<E> {
    type IntoIter = Vec<PathBuf>;

    fn watch_roots(&self) -> Vec<PathBuf> {
        self.include_patterns.iter().map(PathBuf::from).collect()
    }

    /// Matches `path` against the same include and exclude patterns [`Self::paths`] applies, but
    /// against one path instead of globbing the tree. Returns `None` when the path is not valid
    /// UTF-8, since the patterns are compared as strings.
    fn is_included(&self, path: &Path) -> Option<bool> {
        // `paths()` yields candidates in whatever spelling the include pattern used -- `glob` keeps a
        // relative pattern relative and an absolute one absolute (verified) -- and then compares
        // those candidates against the raw exclusion patterns. Notify, by contrast, always reports
        // absolute paths. So each include pattern is matched in *both* spellings, and the exclusions
        // are then applied to the candidate spelling that pattern would have produced. Comparing a
        // single spelling against everything is what previously made this method disagree with the
        // full pass: an excluded short file could be fingerprinted (and then deleted by
        // `remove_after`), or an included one hidden.
        let cwd = std::env::current_dir().ok();
        // Normalized on both sides below: `Pattern` compares component text literally, so a valid
        // `./logs/*.log` include would not match the `<cwd>/logs/app.log` a backend reports.
        let absolute =
            file_source_common::lexically_normalize(&crate::absolutize(path, cwd.as_deref()));
        let absolute_str = absolute.to_str()?;

        // `glob_with` forces `require_literal_separator` regardless of the configured options (see
        // glob's docs), so `*` must not cross a separator here either.
        let include_options = MatchOptions {
            require_literal_separator: true,
            ..self.glob_match_options
        };

        let excluded_in = |candidate: &str| {
            self.exclude_patterns
                .iter()
                .any(|exclude_pattern| exclude_pattern.matches(candidate))
        };

        // Every matching include is considered, not just the first: `paths()` unions the survivors
        // of each pattern, so a path excluded under one pattern's spelling can still be yielded by
        // another. Returning on the first exclusion made this stricter than the full pass.
        // A pattern this method cannot resolve makes the whole answer undecidable, whichever way its
        // own comparison would have gone, so it is checked before any verdict is returned.
        let mut undecidable = false;
        for include_pattern in &self.include_patterns {
            let absolutized = crate::absolutize(Path::new(include_pattern), cwd.as_deref());
            if has_unresolvable_parent_dir(&absolutized) {
                undecidable = true;
                continue;
            }
            let absolute_pattern = normalize_include_pattern(&absolutized);
            let matches_include = absolute_pattern.to_str().is_some_and(|pattern| {
                Pattern::new(pattern)
                    .is_ok_and(|pattern| pattern.matches_with(absolute_str, include_options))
            });
            if !matches_include {
                continue;
            }

            // The candidate spelling this pattern's own glob would have emitted: its literal
            // prefix, with the rest of the event path appended. `paths()` compares *that* against
            // the raw exclusions, so anything else disagrees -- an absolute spelling misses a
            // relative exclusion, and a normalized one misses a dotted exclusion.
            let candidate = candidate_spelling(include_pattern, &absolute_pattern, &absolute);
            let candidate = candidate.as_deref().and_then(Path::to_str);
            if !excluded_in(candidate.unwrap_or(absolute_str)) {
                return Some(true);
            }
        }
        if undecidable {
            return None;
        }

        // Either nothing matched, or every matching pattern's candidate was excluded. Both mean the
        // full pass would not yield this path.
        Some(false)
    }

    fn paths(&self) -> Self::IntoIter {
        self.include_patterns
            .iter()
            .flat_map(|include_pattern| {
                glob::glob_with(include_pattern.as_str(), self.glob_match_options)
                    .expect("failed to read glob pattern")
                    .filter_map(|val| {
                        val.map_err(|error| {
                            self.emitter
                                .emit_path_globbing_failed(error.path(), error.error())
                        })
                        .ok()
                    })
            })
            .filter(|candidate_path: &PathBuf| -> bool {
                !self.exclude_patterns.iter().any(|exclude_pattern| {
                    let candidate_path_str = candidate_path.to_str().unwrap();
                    exclude_pattern.matches(candidate_path_str)
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use file_source_common::internal_events::FileSourceInternalEvents;

    #[derive(Clone)]
    struct NoopEmitter;

    impl FileSourceInternalEvents for NoopEmitter {
        fn emit_file_added(&self, _path: &Path) {}
        fn emit_file_resumed(&self, _path: &Path, _file_position: u64) {}
        fn emit_file_watch_error(&self, _path: &Path, _error: std::io::Error) {}
        fn emit_file_unwatched(&self, _path: &Path, _reached_eof: bool) {}
        fn emit_file_deleted(&self, _path: &Path) {}
        fn emit_file_delete_error(&self, _path: &Path, _error: std::io::Error) {}
        fn emit_file_fingerprint_read_error(&self, _path: &Path, _error: std::io::Error) {}
        fn emit_file_checkpointed(&self, _count: usize, _duration: std::time::Duration) {}
        fn emit_file_checksum_failed(&self, _path: &Path) {}
        fn emit_file_checkpoint_write_error(&self, _error: std::io::Error) {}
        fn emit_files_open(&self, _count: usize) {}
        fn emit_files_idle(&self, _count: usize) {}
        fn emit_path_globbing_failed(&self, _path: &Path, _error: &std::io::Error) {}
        fn emit_file_line_too_long(&self, _buf: &bytes::BytesMut, _max: usize, _size: usize) {}
    }

    fn glob_for(include: &[&str], exclude: &[&str]) -> Glob<NoopEmitter> {
        Glob::new(
            &include.iter().map(PathBuf::from).collect::<Vec<_>>(),
            &exclude.iter().map(PathBuf::from).collect::<Vec<_>>(),
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap()
    }

    /// `is_included` exists so event-driven discovery can decide membership for one path without
    /// globbing the whole include tree on every notify event. It must agree with what `paths()`
    /// would yield, in particular about exclusions -- an excluded short file that slips through
    /// gets added to `known_small_files` and then deleted by `remove_after`.
    #[test]
    fn is_included_matches_include_and_exclude_patterns() {
        let provider = glob_for(&["/logs/*.log"], &["/logs/secret*.log"]);

        assert_eq!(provider.is_included(Path::new("/logs/app.log")), Some(true));
        assert_eq!(
            provider.is_included(Path::new("/logs/secret.log")),
            Some(false),
            "an excluded path must never be reported as included"
        );
        assert_eq!(
            provider.is_included(Path::new("/logs/app.txt")),
            Some(false),
            "a path outside the include pattern is not included"
        );
        assert_eq!(
            provider.is_included(Path::new("/other/app.log")),
            Some(false),
            "a path under an unrelated directory is not included"
        );
    }

    #[test]
    fn is_included_matches_an_absolute_event_path_against_a_relative_pattern() {
        // Regression test for a bug found in review: `include: ["logs/*.log"]` is a valid relative
        // pattern, but notify always reports absolute paths. Comparing the raw strings made a
        // genuinely included new file look excluded, so the targeted pass treated it as accounted
        // for and it went undiscovered until the reconciliation interval -- or was lost entirely if
        // it disappeared first.
        let cwd = std::env::current_dir().unwrap();
        // Created under the test process's CWD so a relative pattern can actually be built; the
        // system temp directory usually is not under CWD.
        let dir = tempfile::tempdir_in(&cwd).unwrap();
        let relative_dir = dir.path().strip_prefix(&cwd).expect("created under cwd");
        let relative_pattern = relative_dir.join("*.log");

        let provider = glob_for(&[relative_pattern.to_str().unwrap()], &[]);

        let absolute_path = dir.path().join("app.log");
        assert!(absolute_path.is_absolute());
        assert_eq!(
            provider.is_included(&absolute_path),
            Some(true),
            "an absolute event path must match the relative include pattern it falls under"
        );

        let absolute_excluded = dir.path().join("app.txt");
        assert_eq!(
            provider.is_included(&absolute_excluded),
            Some(false),
            "a non-matching extension is still excluded"
        );
    }

    #[test]
    fn is_included_requires_literal_separators_like_paths_does() {
        // Regression test for a bug found in review: `paths()` globs with `glob_with`, which forces
        // `require_literal_separator: true`, while this method used the default options -- so `*`
        // crossed directory separators here but not there. The targeted pass would then accept a
        // file nested deeper than the pattern allows, which the full pass omits.
        let provider = glob_for(&["/logs/*.log"], &[]);

        assert_eq!(provider.is_included(Path::new("/logs/app.log")), Some(true));
        assert_eq!(
            provider.is_included(Path::new("/logs/nested/app.log")),
            Some(false),
            "a single `*` must not cross a directory separator, matching glob_with's behavior"
        );

        // `**` is how a pattern opts into crossing separators, in both methods.
        let recursive = glob_for(&["/logs/**/*.log"], &[]);
        assert_eq!(
            recursive.is_included(Path::new("/logs/nested/app.log")),
            Some(true),
            "`**` must still match across separators"
        );
    }

    #[test]
    fn is_included_applies_relative_exclusions_to_an_absolute_event_path() {
        // Regression test for a bug found in review: with a relative include and a relative
        // exclusion, notify supplies an absolute event path. Comparing that absolute path against
        // the relative exclusion never matched, so the targeted pass would fingerprint an excluded
        // short file -- which `remove_after` then deletes, despite the exclusion.
        let cwd = std::env::current_dir().unwrap();
        let dir = tempfile::tempdir_in(&cwd).unwrap();
        let relative_dir = dir.path().strip_prefix(&cwd).expect("created under cwd");

        let provider = glob_for(
            &[relative_dir.join("*.log").to_str().unwrap()],
            &[relative_dir.join("secret*.log").to_str().unwrap()],
        );

        // `paths()` is the authority; `is_included` must agree with it on both files.
        let included = dir.path().join("app.log");
        let excluded = dir.path().join("secret.log");
        std::fs::write(&included, b"x\n").unwrap();
        std::fs::write(&excluded, b"x\n").unwrap();
        let yielded = provider.paths();
        assert_eq!(
            yielded.len(),
            1,
            "paths() must yield only the included file"
        );

        assert_eq!(
            provider.is_included(&included),
            Some(true),
            "an absolute event path under a relative include must be included"
        );
        assert_eq!(
            provider.is_included(&excluded),
            Some(false),
            "a relative exclusion must still apply to the absolute event path"
        );
    }

    #[test]
    fn is_included_normalizes_dot_components_in_patterns() {
        // Regression test for a bug found in review: `Pattern` compares component text literally, so
        // a valid `./logs/*.log` include built a `<cwd>/./logs/*.log` pattern that never matched the
        // `<cwd>/logs/app.log` a notify backend reports (verified against glob). `is_included` then
        // said `Some(false)`, so the targeted pass treated the event as accounted for and the file
        // went undiscovered until the backstop.
        let cwd = std::env::current_dir().unwrap();
        let dir = tempfile::tempdir_in(&cwd).unwrap();
        let relative_dir = dir.path().strip_prefix(&cwd).expect("created under cwd");

        let dotted = Path::new(".").join(relative_dir).join("*.log");
        let provider = glob_for(&[dotted.to_str().unwrap()], &[]);

        let event_path = dir.path().join("app.log");
        assert_eq!(
            provider.is_included(&event_path),
            Some(true),
            "a `./`-prefixed include must match the normalized path a backend reports"
        );

        // `..` must resolve too, and must not accidentally widen the match.
        let parent_hop = Path::new(relative_dir)
            .join("..")
            .join(relative_dir.file_name().unwrap())
            .join("*.log");
        let provider = glob_for(&[parent_hop.to_str().unwrap()], &[]);
        assert_eq!(
            provider.is_included(&event_path),
            Some(true),
            "a `..` hop that lands back on the same directory must still match"
        );
        assert_eq!(
            provider.is_included(&cwd.join("elsewhere.log")),
            Some(false),
            "normalization must not widen the pattern to unrelated paths"
        );
    }

    /// Property the caller depends on: `is_included` must agree with `paths()` for every spelling
    /// combination of include and exclude. Each disagreement found in review was one of these cells,
    /// so they are checked exhaustively rather than one at a time.
    /// The bare-pattern case the spelling matrix missed: with no directory prefix at all, `glob`
    /// emits `secret.log` and a relative exclusion must still match it. Normalizing the empty prefix
    /// to `.` built `./secret.log` instead and let an excluded file through -- where, if it is short
    /// or unterminated, `remove_after` then deletes it.
    #[test]
    fn is_included_applies_exclusions_to_a_bare_pattern() {
        let cwd = std::env::current_dir().unwrap();
        let dir = tempfile::tempdir_in(&cwd).unwrap();
        // The provider's patterns are relative to the process CWD, so run the comparison against
        // files placed directly in it.
        let unique = dir.path().file_name().unwrap().to_str().unwrap().to_owned();
        let included = cwd.join(format!("{unique}-app.log"));
        let excluded = cwd.join(format!("{unique}-secret.log"));
        std::fs::write(&included, b"x\n").unwrap();
        std::fs::write(&excluded, b"x\n").unwrap();
        // Clean up even if an assertion fails.
        struct Cleanup(Vec<std::path::PathBuf>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                for path in &self.0 {
                    drop(std::fs::remove_file(path));
                }
            }
        }
        let _cleanup = Cleanup(vec![included.clone(), excluded.clone()]);

        let provider = glob_for(
            &[&format!("{unique}-*.log")],
            &[&format!("{unique}-secret*.log")],
        );

        let yielded = provider.paths();
        assert_eq!(
            yielded.len(),
            1,
            "paths() must yield only the included file: {yielded:?}"
        );
        assert_eq!(provider.is_included(&included), Some(true));
        assert_eq!(
            provider.is_included(&excluded),
            Some(false),
            "a relative exclusion must match a bare pattern's candidate spelling"
        );

        // The same with an explicit `./` prefix: glob still emits the bare name, so the candidate
        // spelling must not be `./secret.log`.
        let provider = glob_for(
            &[&format!("./{unique}-*.log")],
            &[&format!("{unique}-secret*.log")],
        );
        assert_eq!(
            provider.paths().len(),
            1,
            "paths() must filter the excluded file for a `./` pattern too: {:?}",
            provider.paths()
        );
        assert_eq!(
            provider.is_included(&excluded),
            Some(false),
            "an all-dot prefix must reduce to an empty one, or the exclusion misses"
        );
    }

    /// Regression test for a bug found in review: `glob` drops `.` from the candidates it emits but
    /// keeps `..` (both measured). Normalizing `..` away here too made a `..`-spelled exclusion fail
    /// to match its own file, so an excluded short file could be fingerprinted and then deleted by
    /// `remove_after`.
    #[test]
    fn is_included_keeps_parent_dir_spelling_for_exclusions() {
        let cwd = std::env::current_dir().unwrap();
        let dir = tempfile::tempdir_in(&cwd).unwrap();
        let relative_dir = dir.path().strip_prefix(&cwd).expect("created under cwd");
        let leaf = relative_dir.file_name().unwrap();

        // `<dir>/../<dir>/*.log`: a `..` hop that lands back where it started.
        let include = relative_dir.join("..").join(leaf).join("*.log");
        let exclude = relative_dir.join("..").join(leaf).join("secret*.log");
        let provider = glob_for(&[include.to_str().unwrap()], &[exclude.to_str().unwrap()]);

        let included = dir.path().join("app.log");
        let excluded = dir.path().join("secret.log");
        std::fs::write(
            &included, b"x
",
        )
        .unwrap();
        std::fs::write(
            &excluded, b"x
",
        )
        .unwrap();

        // `paths()` is the authority: it must yield one file, and `is_included` must agree.
        assert_eq!(
            provider.paths().len(),
            1,
            "paths() must filter the excluded file: {:?}",
            provider.paths()
        );
        assert_eq!(provider.is_included(&included), Some(true));
        assert_eq!(
            provider.is_included(&excluded),
            Some(false),
            "a `..`-spelled exclusion must match the candidate spelling glob emits"
        );
    }

    #[test]
    fn is_included_agrees_with_paths_across_spellings() {
        let cwd = std::env::current_dir().unwrap();
        let dir = tempfile::tempdir_in(&cwd).unwrap();
        let relative_dir = dir.path().strip_prefix(&cwd).expect("created under cwd");

        let included = dir.path().join("app.log");
        let excluded = dir.path().join("secret.log");
        std::fs::write(&included, b"x\n").unwrap();
        std::fs::write(&excluded, b"x\n").unwrap();

        // Every way of writing the same directory: relative, `./`-prefixed, and absolute.
        let spellings = [
            relative_dir.to_path_buf(),
            Path::new(".").join(relative_dir),
            dir.path().to_path_buf(),
        ];

        for include_dir in &spellings {
            for exclude_dir in &spellings {
                let provider = glob_for(
                    &[include_dir.join("*.log").to_str().unwrap()],
                    &[exclude_dir.join("secret*.log").to_str().unwrap()],
                );

                let yielded = provider.paths();
                let yields_included = yielded.iter().any(|path| {
                    file_source_common::normalize_path_key(path)
                        == file_source_common::normalize_path_key(&included)
                });
                let yields_excluded = yielded.iter().any(|path| {
                    file_source_common::normalize_path_key(path)
                        == file_source_common::normalize_path_key(&excluded)
                });

                assert_eq!(
                    provider.is_included(&included),
                    Some(yields_included),
                    "include {include_dir:?} / exclude {exclude_dir:?}: disagreed with paths()                      about the included file"
                );
                assert_eq!(
                    provider.is_included(&excluded),
                    Some(yields_excluded),
                    "include {include_dir:?} / exclude {exclude_dir:?}: disagreed with paths()                      about the excluded file"
                );
            }
        }
    }

    #[test]
    fn is_included_agrees_with_paths_about_exclusions() {
        // Guards the property the caller depends on: anything `is_included` accepts, `paths()`
        // would have yielded too, so the targeted pass never fingerprints a file the full pass
        // would have skipped.
        let dir = tempfile::tempdir().unwrap();
        let included = dir.path().join("app.log");
        let excluded = dir.path().join("secret.log");
        std::fs::write(&included, b"x\n").unwrap();
        std::fs::write(&excluded, b"x\n").unwrap();

        let provider = glob_for(
            &[dir.path().join("*.log").to_str().unwrap()],
            &[dir.path().join("secret*.log").to_str().unwrap()],
        );

        let yielded = provider.paths();
        assert!(yielded.contains(&included));
        assert!(!yielded.contains(&excluded));
        assert_eq!(provider.is_included(&included), Some(true));
        assert_eq!(provider.is_included(&excluded), Some(false));
    }

    /// Regression test for a bug found in review: normalizing an include pattern folded `..` into a
    /// preceding wildcard, so `<root>/*/../*.log` became `<root>/*.log`. Glob expansion never does
    /// that -- `*` expands to real directory entries and the `..` stays -- so `is_included` claimed a
    /// path the full scan does not yield, and a notify event for it became eligible for
    /// `remove_after` cleanup.
    ///
    /// Measured both ways: with a subdirectory present glob emits `<root>/sub/../x.log`; with none it
    /// emits nothing at all, which is the case asserted here.
    #[test]
    fn is_included_does_not_fold_parent_dir_into_a_wildcard() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("x.log");
        std::fs::write(&file, b"line\n").unwrap();
        // No subdirectory exists, so `*` can only match a file and the glob yields nothing.

        let pattern = directory.path().join("*").join("..").join("*.log");
        let provider = Glob::new(
            std::slice::from_ref(&pattern),
            &[],
            MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();

        assert!(
            provider.paths().is_empty(),
            "test setup requires the full scan to yield nothing: {:?}",
            provider.paths()
        );
        assert_ne!(
            provider.is_included(&file),
            Some(true),
            "is_included must not claim a path the full scan never yields, or remove_after can \
             delete a file outside the configured glob"
        );
    }

    #[cfg(unix)]
    #[test]
    fn is_included_defers_when_a_parent_dir_cancels_a_symlink() {
        // The kernel resolves the link before applying `..`, so the hop lands in the *target's*
        // parent. Measured: with `<dir>/link` -> `<dir>/nested/deep`, `<dir>/link/../*.log` opens
        // files in `<dir>/nested`, while a lexical pop would claim `<dir>/*.log`.
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("nested");
        let deep = nested.join("deep");
        std::fs::create_dir_all(&deep).unwrap();
        // The file the folded pattern would wrongly claim.
        let outside = directory.path().join("x.log");
        std::fs::write(&outside, b"line\n").unwrap();
        // The file the real hop reaches.
        std::fs::write(nested.join("x.log"), b"line\n").unwrap();
        std::os::unix::fs::symlink(&deep, directory.path().join("link")).unwrap();

        let pattern = directory.path().join("link").join("..").join("*.log");
        let provider = Glob::new(
            std::slice::from_ref(&pattern),
            &[],
            MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();

        assert!(
            !provider.paths().iter().any(|path| path == &outside),
            "test setup requires the full scan not to yield the file outside the link's target: {:?}",
            provider.paths()
        );
        assert_eq!(
            provider.is_included(&outside),
            None,
            "a `..` that cancels a symlink cannot be folded lexically, so is_included must defer \
             rather than claim a path the scan never yields"
        );
    }

    /// Folding `<root>/missing/../*.log` assumes `missing` is a directory. Measured: glob yields
    /// nothing when it is absent, and nothing when a file sits there -- while the folded pattern
    /// matched `<root>/x.log`, so `remove_after` could delete a path outside the configured glob.
    #[test]
    fn is_included_defers_when_a_parent_dir_cancels_a_missing_directory() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("x.log");
        std::fs::write(&file, b"line\n").unwrap();
        // A *file* where the pattern expects a directory to traverse.
        std::fs::write(directory.path().join("blocker"), b"").unwrap();

        for hop in ["missing", "blocker"] {
            let pattern = directory.path().join(hop).join("..").join("*.log");
            let provider = Glob::new(
                std::slice::from_ref(&pattern),
                &[],
                MatchOptions::default(),
                NoopEmitter,
            )
            .unwrap();

            assert!(
                provider.paths().is_empty(),
                "setup requires the full scan to yield nothing for {hop}: {:?}",
                provider.paths()
            );
            assert_eq!(
                provider.is_included(&file),
                None,
                "`{hop}/..` cannot be folded away, so is_included must defer rather than claim the \
                 path is included"
            );
        }
    }

    /// The other half of the wildcard case: with a directory for it to match, the full scan yields the
    /// file while the event path does not match the pattern. Answering `Some(false)` there discarded
    /// the event, leaving a genuinely included file unread until the backstop.
    #[test]
    fn is_included_defers_when_a_wildcard_pattern_has_an_ambiguous_spelling() {
        let directory = tempfile::tempdir().unwrap();
        // The subdirectory the wildcard matches, so the full scan does yield the file.
        std::fs::create_dir(directory.path().join("a")).unwrap();
        let file = directory.path().join("x.log");
        std::fs::write(&file, b"line\n").unwrap();

        let pattern = directory.path().join("*").join("..").join("*.log");
        let provider = Glob::new(
            std::slice::from_ref(&pattern),
            &[],
            MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();

        let yielded = provider.paths();
        assert!(
            !yielded.is_empty(),
            "test setup requires the full scan to yield the file, in its own spelling"
        );
        assert_eq!(
            provider.is_included(&file),
            None,
            "a pattern whose `..` follows a wildcard has no comparable spelling, so is_included \
             must defer to the full pass rather than claiming the path is excluded: {yielded:?}"
        );
    }
}
