# deprecation.d

This directory contains deprecation notices for Vector.

Each file describes a feature, configuration option, or behavior that is being deprecated.
These notices are collected during the release process and rendered into two sections of the
release notes:

- **`deprecations`** – items whose removal version matches the current release (enacted now).
- **`planned_deprecations`** – items scheduled for removal in a future release.

## File format

Each file must be named `<unique_slug>.md` and begin with YAML frontmatter:

````markdown
---
what: "`legacy_auth` configuration option"
deprecation_version: 0.57.0
---

The `legacy_auth` option has been replaced by the new `auth` block.

Migrate by replacing:

```yaml
legacy_auth: "my_token"
```

with:

```yaml
auth:
  token: "my_token"
```
````

### Frontmatter fields

| Field | Required | Description |
| ----- | -------- | ----------- |
| `what` | Yes | Short one-line description of what is deprecated. |
| `deprecation_version` | Yes | Version when the item will be removed. Accepts a semver string (`0.56`, `0.56.0`). Must be greater than the latest released version. |

### Body

The body of the file is an optional Markdown explanation: migration instructions, rationale,
or links to further documentation. It is rendered verbatim in the release notes.

## Lifecycle

1. **Announce** – a PR adds a file to this directory when the deprecation is first introduced.
2. **Planned** – every subsequent release lists the entry under `planned_deprecations`.
3. **Enacted** – when the release version equals `deprecation_version`, the entry moves to
   `deprecations` in the release notes and the file is removed from this directory.

## Validation

Run `cargo vdev check deprecations` to validate all files in this directory. The check also
verifies that each `deprecation_version` is greater than the latest released version — entries
that should have been enacted and removed will cause the check to fail.

To preview the current deprecation state, run `cargo vdev deprecation show`.
