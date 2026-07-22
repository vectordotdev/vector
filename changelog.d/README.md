## Overview

This directory contains changelog "fragments" that are collected during a release to
generate the project's user facing changelog.

The conventions used for this changelog logic follow [towncrier](https://towncrier.readthedocs.io/en/stable/markdown.html).

The changelog fragments are located in `changelog.d/`.

## Quick start

The scaffolder is the recommended workflow for all fragment types, but devs can also hand-write changelog fragments.

Install `vdev`:

    cargo binstall --manifest-path vdev/Cargo.toml vdev
    # or
    # cargo install vdev
    # or use the prefix
    # cargo vdev <command>

Then scaffold a fragment:

    vdev changelog new <type> <slug>

> `vdev` fills in the filename, the required structure, and your authors line (auto-detected from `git config github.user`, `gh api user`, or a `users.noreply.github.com` email)

Edit the file and validate:

    vdev check changelog-fragments

Examples:

    vdev changelog new fix 42_kafka_ack_race
    vdev changelog new enhancement retry_backoff_config
    vdev changelog new breaking env_var_interpolation

## Process

Fragments for un-released changes are placed in the root of this directory during PRs.

During a release when the changelog is generated, the fragments in the root of this
directory are organized into the [releases directory](../website/cue/reference/releases)
with the name of the release (e.g. '0.42.0.cue').

### Pull Requests

By default, PRs are required to add at least one entry to this directory.
This is enforced during CI.

To mark a PR as not requiring user-facing changelog notes, add the label 'no-changelog'.

To run the same check that is run in CI to validate that your changelog fragments have
the correct syntax, commit the fragment additions and then run `vdev check changelog-fragments`.

The format for fragments is: `<unique_name>.<fragment_type>.md`

### Fragment conventions

When fragments used to generate the updated changelog, the content of the fragment file is
rendered as an item in a bulleted list under the "type" of fragment.

The contents of the file must be valid markdown.

Filename rules:

- The first segment (unique_name) should be a unique string related to the change.
  Optionally, if there is a GitHub issue associated with the change, it can be used as a prefix.
  For example `42_very_important_change.breaking.md`, vs `very_important_change.breaking.md`.
- The type must be one of the valid types in [Fragment types](#fragment-types)
- Only the two period delimiters can be used.
- The file must be markdown.

#### Fragment types

- `breaking`: A change that is incompatible with prior versions which requires users to make adjustments.
- `security`: A change that is has implications for security.
- `feature`: A change that is introducing a new feature.
- `enhancement`: A change that is enhancing existing functionality in a user perceivable way.
- `fix`: A change that is fixing a bug.

#### Fragment contents

When fragments are rendered in the changelog, each fragment becomes an item in a markdown list.
For this reason, when creating the content in a fragment, the format must be renderable as a markdown list.

As an example, separating content with markdown header syntax should be avoided, as that will render
as a heading in the main changelog and not the list. Instead, separate content with newlines.

### Breaking changes

Breaking fragments (`*.breaking.md`) carry extra structured fields (title, optional anchor, and
`## Summary` / `## Migration` sections) so the release process can auto-generate the upgrade
guide from them. See [Examples](#examples) below for the exact shape — or just run the
scaffolder from [Quick start](#quick-start).

## Community Contributors

When a PR is authored/has commits by a contributor from the Vector community, the fragment contents
can optionally contain a line which specifies the community members involved in making the change.
This is later used during the release process to render as a link to the github user profile for
the authors specified.

The process for adding this is simply to have the last line of the file be in this format:

    authors: <author1_gh_username> <author2_gh_username> <...>

Do not include a leading `@` when specifying your username.

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
backlink) followed by `## Summary` and `## Migration` sections. Only the `## Summary` content
lands in the changelog list; the title, anchor, and `## Migration` body feed the
auto-generated upgrade guide.

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
