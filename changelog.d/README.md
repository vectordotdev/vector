## Overview

This directory contains changelog "fragments" that are collected during a release to
generate the project's user facing changelog.

The tool used to generate the changelog is [towncrier](https://towncrier.readthedocs.io/en/stable/markdown.html).

The configuration file is `changelog.toml`.
The changelog fragments are are located in `changelog.d/`.

## Process

Fragments for un-released changes are placed in the root of this directory during PRs.

During a release when the changelog is generated, the fragments in the root of this
directory are moved into a new directory with the name of the release (e.g. '0.42.0').

### Pull Requests

By default, PRs are required to add at least one entry to this directory.
This is enforced during CI.

To mark a PR as not requiring user-facing changelog notes, add the label 'no-changelog'.

To run the same check that is run in CI to validate that your changelog fragments have
the correct syntax, commit the fragment additions and then run ./scripts/check_changelog_fragments.sh

The format for fragments is: \<pr_number\>.\<fragment_type\>.md

### Fragment conventions

When fragments used to generate the updated changelog, the content of the fragment file is
rendered as an item in a bulleted list under the "type" of fragment.

The contents of the file must be valid markdown.

Filename rules:
- Must begin with the PR number associated with the change.
- The type must be one of: breaking|security|deprecated|feature|enhanced|fixed.
  These types are described in more detail in the config file (see `changelog.toml`).
- Only the two period delimiters can be used.
- The file must be markdown.

### Breaking changes

When using the type 'breaking' to add notes for a breaking change, these should be more verbose than
other entries typically. It should include all details that would be relevant for the user to need
to handle upgrading to the breaking change.

## Example

Here is an example of a changelog fragment that adds a breaking change explanation.

    $ cat changelog.d/42.breaking.md
    This change is so great. It's such a great change that this sentence
    explaining the change has to span multiple lines of text.

    It even necessitates a line break. It is a breaking change after all.

This renders in the auto generated changelog as:
(note that PR links are omitted in the public facing version of the changelog)

    ## [X.X.X]

    ### Breaking Changes & Upgrade Guide

    - This change is so great. It's such a great change that this sentence
      explaining the change has to span multiple lines of text.

      It even necessitates a line break. It is a breaking change after all. ([#42])(https://github.com/vectordotdev/vector/pull/42))
