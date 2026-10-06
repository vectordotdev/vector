## Overview

This directory contains changelog "fragments" that are collected during a release to
generate the project's user-facing changelog.

The conventions used for this changelog logic follow [towncrier](https://towncrier.readthedocs.io/en/stable/markdown.html).

The changelog fragments are located in `changelog.d/`.

## Prerequisites

Check whether [vdev](https://crates.io/crates/vdev) is installed, and that it is version
0.3.15 or newer:

    vdev --version

If not:

    cargo binstall --manifest-path vdev/Cargo.toml vdev
    # or
    # cargo install vdev
    # or use the prefix
    # cargo vdev <command>

## Quick start

The scaffolder is the recommended workflow for all fragment types, but devs can also hand-write changelog fragments.

Scaffold a fragment:

    vdev changelog new <type> <slug>

> `vdev` fills in the filename, the required structure, and your authors line (auto-detected from `git config github.user`, `gh api user`, or a `users.noreply.github.com` email)

Edit the file and validate with:

    vdev check changelog-fragments

Examples:

    vdev changelog new fix 42_kafka_ack_race
    vdev changelog new enhancement retry_backoff_config
    vdev changelog new breaking env_var_interpolation

### When do I need a fragment?

Add a fragment when the change is user-observable: it alters behavior, configuration, output
format, performance, or security posture that a Vector user would notice.

Skip the fragment (and add the `no-changelog` label) for internal-only changes: refactors with
no behavior change, CI/test tooling, documentation, or dependency bumps that don't affect behavior.

## Process

Fragments for unreleased changes are placed in the root of this directory during PRs.

During a release when the changelog is generated, the fragments in the root of this
directory are organized into the [releases directory](../website/cue/reference/releases)
with the name of the release (e.g. '0.42.0.cue').

### Pull Requests

By default, PRs are required to add at least one entry to this directory.
This is enforced during CI.

To mark a PR as not requiring user-facing changelog notes, add the label 'no-changelog'.

To validate your changelog fragments the same way CI does, commit the fragment additions and then run
`vdev check changelog-fragments`.
It validates the filename format, the `authors:` line, and the breaking-fragment structure
(`## Summary` / `## Migration`).

The format for fragments is: `<unique_name>.<fragment_type>.md`

### Fragment conventions

When fragments are used to generate the updated changelog, the content of the fragment file is
rendered as an item in a bulleted list under the "type" of fragment.

The contents of the file must be valid markdown.

Filename rules:

- The first segment (unique_name) should be a unique string related to the change.
  Optionally, if there is a GitHub issue associated with the change, it can be used as a prefix.
  For example, `42_very_important_change.breaking.md` vs `very_important_change.breaking.md`.
- The type must be one of the valid types reported by `vdev changelog types`.
- The filename must contain exactly two periods (separating the name, type, and extension).
- The file must be markdown.

#### Fragment types

The valid fragment types and their descriptions are defined by `vdev changelog types`:

    $ vdev changelog types
    breaking     A change that is incompatible with prior versions and requires users to make adjustments. If a change is also a fix or feature, breaking takes precedence.
    security     A change that has security implications.
    feature      A change that introduces a new feature.
    enhancement  A change that enhances existing functionality in a user perceivable way.
    fix          A change that fixes a bug.

#### Fragment contents

When fragments are rendered in the changelog, each fragment becomes an item in a markdown list.
For this reason, when creating the content in a fragment, the format must be renderable as a markdown list.

For example, avoid separating content with markdown header syntax, as it will render
as a heading in the main changelog rather than a list item. Instead, separate content with newlines.

A good fragment answers three questions:

1. How does this change affect user-visible behavior?
2. Which components are affected?
3. Which config fields are introduced or affected?

Finally, a good fragment is concise and avoids implementation details.

### Breaking changes

Breaking fragments (`*.breaking.md`) carry extra structured fields (title, optional anchor, and
`## Summary` / `## Migration` sections) so the release process can auto-generate the upgrade
guide from them. See [Examples](#examples) below for the exact shape — or just run the
scaffolder from [Quick start](#quick-start).

## Authors

Every fragment must end with an `authors:` line:

    authors: <author1_gh_username> <author2_gh_username> <...>

Do not prefix usernames with `@`.

## Examples

### Non-breaking

`fix`, `feature`, `enhancement`, and `security` fragments are free-form markdown followed by an
`authors:` line. The whole body becomes a single bullet in the release changelog list, so avoid
markdown headings inside the body.

    $ cat changelog.d/42_kafka_ack_race.fix.md
    Fix a race in the kafka source where offsets could be committed before acknowledgements were
    flushed. This resurfaced under high partition rebalance frequency.

    authors: some_contributor

### Breaking

Breaking fragments start with an H1 title (optionally with a Hugo-style `{#anchor}` for a stable
backlink) followed by `## Summary` and `## Migration` sections. The `## Summary` content lands in
the changelog list; the title and anchor are also rendered on the release page as links into the
auto-generated upgrade guide, which uses the title, anchor, and `## Migration` body.

    $ cat changelog.d/env_var_interpolation.breaking.md
    # Environment variable interpolation disabled by default {#env-var-interpolation}

    ## Summary

    Environment variable interpolation in configuration files is now disabled by default.
    The `--disable-env-var-interpolation` flag and `VECTOR_DISABLE_ENV_VAR_INTERPOLATION`
    environment variable have been removed.

    ## Migration

    Pass `--dangerously-allow-env-var-interpolation` (or set
    `VECTOR_DANGEROUSLY_ALLOW_ENV_VAR_INTERPOLATION=true`) on startup to restore the previous
    behavior:

    #### Old

    ```bash
    vector --config vector.yaml
    ```

    #### New

    ```bash
    vector --config vector.yaml --dangerously-allow-env-var-interpolation
    ```

    authors: some_contributor

Put `N/A` under `## Migration` for informational-only breakers with nothing to do on the user's
side.
