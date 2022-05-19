---
title: "VRL iteration support has arrived!"
short: "VRL iteration"
description: "Remap all your collections using iteration functions."
authors: ["JeanMertz"]
date: "2022-05-18"
pr_numbers: [12317]
release: "0.22.0"
hide_on_release_notes: false
badges:
  type: announcement
  domains: ["vrl", "remap"]
tags: ["vector remap language", "vrl", "dsl", "iteration", "enumeration"]
---

Since we launched **Vector Remap Language** (VRL) [more than a year ago][], the
language has seen a massive growth in both usage, and feature richness. While we
intentionally left the core language in a stable state since its launch, we
_have_ been steadily expanding the [standard library][], increasing
[performance][], adding [event enrichment features][], and [squashing many
bugs][].

One of the last major pieces missing from the language — and one requested by
many of you — is support for mapping collections. We are happy to announce that
as of Vector 0.22.0, this feature is now available.

Let’s take a look at iteration in VRL, and how it works.

## Usage

Let’s take a simple example:

```json
{
  "foo": true,
  "bar": { "bar": "" },
  "baz": "",
  "qux": ["a", "b", "a", "c"]
}
```

Let’s assume we want to achieve the following results:

1. Upcase all keys in the object.
2. Change all empty string values to `null`.
3. Count the number of equal elements in the `qux` array.

Here’s how we would solve these individual tasks:

```coffee
# 1. Upcase all keys in the object.
. = map_keys(., recursive: true) -> |key| { upcase(key) }

# 2. Change all empty string values to `null`.
. = map_values(., recursive: true) -> |value| {
  if value == ”” { null } else { value }
}

# 3. Count the number of equal elements in the `qux` array.
.qux_tally = {}
for_each(.qux) -> |_index, value| {
  tally = int(get!(.qux_tally, [value])) ?? 0

  .qux_tally = set!(.qux_tally, [value], tally + 1)
}
```

Running this VRL program results in the following output:

```json
{
  "FOO": true,
  "BAR": { "BAR": null },
  "BAZ": null,
  "QUX": ["a", "b", "a", "c"],
  "qux_tally": {
    "a": 2,
    "b": 1,
    "c": 1
}
```

## Design Details

As you can see, VRL iteration happens through regular _function calls_, combined
with the new _closure syntax_. We chose this solution for a few reasons:

1. Allows us to expand the standard library with more specialized iteration
   functions going forward.
2. Prevents introducing accidental infinite recursions to your program.
3. Provides enough flexibility for manipulating observability data, without
   adding overly complex special-purpose iteration syntax.

The three functions we introduce in this release (`map_keys`, `map_values`, and
`for_each`) serve as a good starting point for any generic iteration logic, but
we already have a list of special-purpose iteration functions available in the
standard library, and plan to add more, as long as they fit the purpose of the
language, remapping observability data.

Take a look at our [existing iteration functions][], and feel free to [file
a request][] for any special purpose function that would make your program
easier to maintain and/or more performant to run.

We hope you are as happy with this enhancement to VRL as we are. Iteration
support unlocks one of the last areas for which many of you had to fall back to
the [LUA runtime transform][]. You can now keep your remapping logic in VRL,
with all the performance and runtime correctness guarantees VRL delivers.

Please [join us on Discord][] or tweet at us at [@vectordotdev][] to discuss
this, or any other aspect of Vector and VRL.

[more than a year ago]: https://vector.dev/blog/vector-remap-language/
[standard library]: https://vrl.dev/functions/
[performance]: https://vector.dev/highlights/2022-03-15-vrl-vm-beta/
[event enrichment features]: https://vector.dev/highlights/2021-11-18-csv-enrichment/
[squashing many bugs]: https://github.com/vectordotdev/vector/issues?q=is%3Aissue+sort%3Aupdated-desc+is%3Aclosed+label%3A%22domain%3A+vrl%22+label%3A%22type%3A+bug%22
[join us on Discord]: https://discord.gg/n3CuBAwNCn
[@vectordotdev]: https://twitter.com/vectordotdev
[existing iteration functions]: https://vrl.dev/functions/#enumerate-functions
[file a request]: https://github.com/vectordotdev/vector/issues/new?labels=type%3A+feature&template=feature.yml
