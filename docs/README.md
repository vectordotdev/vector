# The Vector documentation sources

This directory houses the structured data sources used to build the Vector documentation at
https://vector.dev. These sources are written in the [CUE] language, which is designed for
configuration and data validation.

> Vector is currently using CUE version **0.3.0-beta.5**. Please be sure to use precisely this
> version, as CUE is evolving quickly and you can expect breaking changes in each release.

## How it works

When the HTML output for the Vector docs is built, the `vector` repo is cloned and these CUE sources
are converted into one big JSON object using the `cue export` command, which is then used as an
input to the site build.

## Formatting

Vector has some CUE-related CI checks that are run whenever changes are made to this `docs`
directory. This includes checks to make sure that the CUE sources are properly formatted. To run
CUE's autoformatting, run this command from the `vector` root:

```bash
cue fmt ./docs/**/*.cue
```

If that rewrites any files, make sure to commit your changes or else you'll see CI failures.

## Validation

In addition to proper formatting, the CUE sources need to be *valid*, that is, the provided data
needs to conform to various CUE schemas. To check the validity of the CUE sources:

```bash
make check-docs
```

## Development flow

A good best practice for using CUE is to make small, incremental changes and check to ensure that
those changes are valid. If you introduce larger changes that introduce multiple errors, you may
have difficulty interpreting CUE's verbose (and not always super helpful) log output. In fact, we
recommend using a tool like [watchexec] to validate the sources every time you save a change:

```bash
# From the root
watchexec "make check-docs"
```

[cue]: https://cuelang.org
[watchexec]: https://github.com/watchexec/watchexec
