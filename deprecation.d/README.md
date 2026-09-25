# deprecation.d

This directory contains deprecation notices for Vector.

Each file describes a feature, configuration option, or behavior that is being deprecated.
These notices are collected during the release process and rendered into two sections of the
release notes:

- **`deprecation_announcements`** – items deprecated in this release (announced for the first time).
- **`planned_deprecations`** – items deprecated in an earlier release.

## File format

Each file must be named `<unique_slug>.md` and begin with YAML frontmatter:

````markdown
---
what: "`legacy_auth` configuration option"
deprecated_since: "0.57.0"
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
| `deprecated_since` | Yes | The release version in which this deprecation was first announced. Accepts a semver string (`0.56`, `0.56.0`). |

### Body

The body of the file is an optional Markdown explanation: migration instructions, rationale,
or links to further documentation. It is rendered verbatim in the release notes.

## Lifecycle

1. **Announce** – a PR adds a file to this directory when the deprecation is first introduced.
2. **Planned** – every subsequent release lists the entry under `planned_deprecations`.
3. **Removed** – when a deprecated feature is finally removed, the PR runs
   `cargo vdev deprecation enact <slug> --version <removed-in-version>`. The command
   records the removal in `website/data/deprecations.json` and deletes the fragment in
   one step; deleting the fragment manually would drop it from `past_deprecations`.

## Validation

Run `cargo vdev deprecation check` to validate all files in this directory.

To preview the current deprecation state, run `cargo vdev deprecation show`.
