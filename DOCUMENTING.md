# Documenting

This document covers the basics of writing documentation within Vector.
In this document:

<!-- MarkdownTOC autolink="true" indent="   " -->

- [Prerequisites](#prerequisites)
- [How It Works](#how-it-works)
- [Making Changes](#making-changes)
- [Conventions](#conventions)

<!-- /MarkdownTOC -->


## Prerequisites

1. **You are familiar with the [docs](https://docs.vector.dev).**
2. **You have read the [Contributing](/CONTRIBUTING.md) guide.**
3. **You understand [markdown](https://daringfireball.net/projects/markdown/).**

## How It Works

1. Vector's documentation is located in the [/docs](/docs) folder.
2. All files are in markdown format.
3. The documentation is a mix of hand-written and generated docs.
4. Docs are generated via the `make generate-docs` command which delegates to
   the [`scripts/generate-docs.sh`](/scripts/generate-docs.sh) file.
   1. This is a mix of Ruby scripts that parses the
      [`/.metadata.toml`](/.metadata.toml) file and runs a series of generators.
   2. Each generated section is clearly called out in the markdown file to
      help ensure humans do not modify it:

      ```
      <!-- START: sources_table -->
      <!-- ----------------------------------------------------------------- -->
      <!-- DO NOT MODIFY! This section is generated via `make generate-docs` -->

      ...

      <!-- ----------------------------------------------------------------- -->
      <!-- END: sources_table -->
      ```

## Making Changes

You can edit the markdown files directly in the /docs folder.  Auto-generated
sections are clearly denoted as described above. To make make changes
to auto-generated sections:

1. Modify the `/.metadata.toml` file as necessary.
2. Run `make generate-docs`
3. Commit changes.

## Conventions

See the [Conventions](/docs/meta/conventions.md) doc for details on how to
properly write documentation for Vector.