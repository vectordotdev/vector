# Documenting

Documentation is very important to the Vector project! This document covers the
process of documenting Vector from a contributor's perspective. _All_ Vector
contributors are [responsible](#responsibilities) for maintaining Vector's
documentation in tandem with code changes.

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [Responsibilities](#responsibilities)
2. [Reference documentation](#reference-documentation)
   1. [Installing CUE](#installing-cue)
   2. [Generating from source code](#generating-from-source-code)
   3. [Formatting](#formatting)
   4. [Validating](#validating)
      1. [Tips & tricks](#tips--tricks)
         1. [Make small incremental changes](#make-small-incremental-changes)
   5. [Changelog](#changelog)
   6. [Release highlights](#release-highlights)
      1. [FAQ](#faq)
         1. [What makes a release highlight noteworthy?](#what-makes-a-release-highlight-noteworthy)
         2. [How is a release highlight different from a blog post?](#how-is-a-release-highlight-different-from-a-blog-post)

<!-- /MarkdownTOC -->

## Responsibilities

As a Vector contributor you _are_ responsible for coupling the following user
experience related changes with your code:

- Reference docs changes located in the [`/website/cue` folder](../website/cue) (generally configuration changes)
- Existing guide changes located in the [`/website/content` folder](../website/content)
- If relevant, [highlighting](../website/content/en/highlights) your change for future release notes

By default, you are _not_ responsible for:

- Writing new guides related to your change (unless assigned)
- Writing a blog post on your change (unless assigned)

## Reference documentation

Vector's [reference documentation](https://vector.dev/docs/reference) serves as
a referential index for all things Vector. For example, it contains a
[comprehensive list of options](https://vector.dev/docs/reference/configuration/)
available in Vector's configuration. It's very data-centric and, as a result,
is driven by structured data defined in the [`/website/cue`](../website/cue)
directory. [CUE](https://cuelang.org) is a declarative configuration language
suitable for complex data definitions.

### Installing CUE

Cue can be [installed](https://cuelang.org/docs/install/) from package managers,
however it may be necessary to install it from source in order to use the correct
version that Vector depends on. Currently Vector is using `v0.7.0`. Using a CUE
version different than this may result in CUE check/build errors. We are aiming
to improve the developer experience around external tool dependencies ([#15909](https://github.com/vectordotdev/vector/issues/15909)).

### Generating from source code

Much of Vector's reference documentation is automatically compiled from source code (e.g., doc comments).
To regenerate this content, run:

```bash
make generate-component-docs
```

### Formatting

Vector has some CUE-related CI checks that are run whenever changes are made to
the `docs` directory. This includes checks to make sure that the CUE sources are
properly formatted. To run CUE's autoformatting, first [install cue](https://cuelang.org/docs/install/),
then run this command from the `vector` root:

```bash
cue fmt ./website/**/*.cue
```

If that rewrites any files, make sure to commit your changes or else you'll see
CI failures.

### Validating

In addition to proper formatting, the CUE sources need to be _valid_, that is,
the provided data needs to conform to various CUE schemas. To check the validity
of the CUE sources:

```bash
make check-docs
```

#### Tips & tricks

##### Make small incremental changes

A good practice for writing CUE is to make small, incremental changes and to
frequently check to ensure that those changes are valid. If you introduce larger
changes that introduce multiple errors, you may have difficulty interpreting
CUE's verbose (and not always super helpful) log output. In fact, we recommend
using a tool like [watchexec](https://github.com/watchexec/watchexec) to validate the sources every time you save a
change:

```bash
# From the root
watchexec "make check-docs"
```

### Changelog

Contributors do not need to maintain a changelog. This is automatically generated
via the `make release` command, made possible by the use of
[conventional commit](https://www.conventionalcommits.org/en/v1.0.0/) titles.

### Release highlights

Because Vector releases often contain many different changes, we use highlights
to surface high-value, meaningful changes. Highlights are markdown files located
in the [`docs/content/en/highlights` folder](../website/content/en/highlights) that
thoughtfully describe a feature. Each highlight is prominently displayed in the
relevant [release notes](https://vector.dev/releases/).

#### FAQ

##### What makes a release highlight noteworthy?

It should offer meaningful value to users. This is inherently subjective, and
it is impossible to define exact rules for this distinction. But we should be
cautious not to dilute the meaning of a highlight by producing low values
highlights. Typically, a release contains no more than 6 highlights.

##### How is a release highlight different from a blog post?

Highlights are not blog posts. They are short one, maybe two, paragraph
announcements. Highlights should allude to, or link to, a blog post if
relevant.

For example, [this adaptive concurrency announcement](https://vector.dev/highlights/2020-09-18-adaptive-concurrency)
is noteworthy, but also deserves an in-depth blog post covering the work that
resulted in the performance and reliability benefit.
