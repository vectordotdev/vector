The `file` sink now confines every rendered `path` value to an
operator-controlled base directory, closing a path-traversal vulnerability
where a log producer that controls any field used in `path` could write event
data to arbitrary filesystem locations.

Confinement is enforced at multiple layers:

- **Lexical normalization** — `..` segments and absolute path injections are
  rejected before any I/O.
- **Symlink-safe directory creation** — intermediate directories are created
  without following symlinks (`create_dirs_nofollow`).
- **Final-component symlink check** — the output file is opened with
  `O_NOFOLLOW`, preventing a symlink at the last path segment from redirecting
  writes.
- **NUL byte and length cap** — NUL bytes and paths exceeding 1 KiB are
  rejected.

The base directory is derived automatically from the literal prefix of the
`path` template, or set explicitly via the new `base_dir` config field.

authors: pront
