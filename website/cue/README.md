# The Vector documentation sources

> **Note**: This document covers the specifics of Vector and [CUE]. For a more general guide to
> contributing to Vector, see [`CONTRIBUTING.md`][contrib].

Vector's using-facing [docs] rely heavily on structured data generated using [CUE], a configuration
language with support for data schemas and validation.

## Why we use CUE

Vector is a software system with a great many "knobs." Some examples:

* Vector offers nearly 100 [components] (sources, transforms, and sinks), each of which has many
  configuration parameters and other attributes that need to be displayed on pages [like
  this][aws_s3_source].
* [Vector Remap Language][vrl] (VRL) has over 100 [functions] and dozens of [error codes][errors]
  that need to be documented.

In order to present this all information, we rely heavily on **structured data**. Why? Because more
traditional ways of writing and maintaining docs, like keeping information in plain Markdown files,
proved quite unwieldy. The more modern approach of keeping all this info in a format like [TOML]
was an improvement but suffered from several drawbacks, most notably that the data was hard to
validate and the TOML files were rife with redundant information.

CUE's feature set provided us with a much better approach. With CUE we're able to:

* Define all of our structured data using CUE [schemas], which provide support for things like
  default values, optional values, and type constraints.
* Cut down on data redundancy by, for example, storing values in variables and inserting those
  variables in multiple places.

## How the site is generated

When you run the Vector website locally (e.g. by running `make serve`), two things happen:

* The `cue` CLI tool turns all the `.cue` files under this directory into a single large JSON file
  that's several MB in size.
* The [Hugo] static site generator uses that JSON file to build user-viewable HTML using templates
  like the [`data.html`](../layouts/partials/data.html) template. These templates can be quite
  complex, so don't worry if you can't quite grok them; knowledge of the HTML templating layer is
  *not* necessary for contributing to the CUE docs.

## How our CUE sources are structured

All the CUE files inside this directory act as a single whole; you'll see this whole referred to as "the CUE sources."

## Challenges with CUE

Although does provide a lot of value for Vector's way of doing docs,


[aws_s3_source]: https://vector.dev/docs/reference/configuration/sources/aws_s3
[components]: https://vector.dev/components
[contrib]: ../../CONTRIBUTING.md#documentation
[cue]: https://cuelang.org
[docs]: https://vector.dev/docs
[errors]: https://vrl.dev/errors
[functions]: https://vrl.dev/functions
[hugo]: https://gohugo.io
[schemas]: https://cuelang.org/docs/usecases/datadef
[toml]: https://toml.io
[vrl]: https://vrl.dev
