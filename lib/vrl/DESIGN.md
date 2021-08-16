# Vector Remap Language: Guiding Design Principles

This document describes the high-level goals and directions of the Vector Remap
Language (VRL). It is intended to help guide discussions and decisions made
during the development of VRL itself. The document is less relevant to _users_
of VRL, the [language documentation][docs] exists for that audience.

[docs]: https://vrl.dev

## Table of Contents

* [The Zen of VRL](#the-zen-of-vrl)
* [Why VRL](#why-vrl)
* [Target Audience](#target-audience)
* [Language Limits](#language-limits)
* [Conventions](#conventions)
  * [Functions](#functions)
    * [Composition](#composition)
    * [Naming](#naming)
    * [Return Types](#return-types)
    * [Mutability](#mutability)
    * [Fallibility](#fallibility)
    * [Signatures](#signatures)
  * [Errors](#errors)
* [Patterns](#patterns)
  * [Error Chaining](#error-chaining)

## The Zen of VRL

In the spirit of [The Zen of Python][PEP20].

- Beautiful is better than ugly.
- Explicit is better than implicit.
- Simple is better than complex.
- Sparse is better than dense.
- Readability trumps writing convenience.
- Special cases aren't special enough to break the rules.
- Although practicality beats purity.
- Function calls over syntax.
- Errors may never occur at run-time.
- Unless explicitly marked as such.
- There should be one — and preferably only one — obvious way to do it.
- Performance matters.
- Provide observability remapping solutions, nothing else.

[PEP20]: https://www.python.org/dev/peps/pep-0020/

## Why VRL

VRL exists to solve the need for a _simple_ and _performant_ **domain specific
language** (DSL) to remap observability data.

Its purpose is to hit a sweet spot between Turing complete languages such as
Lua, and static transforms such as Vector's `rename_fields`. It should be
flexible enough to cover most remapping use-cases, without requiring operators
to write complex scripts that are difficult to reason about and incur
a significant performance penalty.

See the [introduction blog post][blog] for more details on the **why**.

[blog]: https://vector.dev/blog/vector-remap-language/#preamble

## Target Audience

VRL has a specialized purpose of solving observability data remapping needs.
Because of this purpose, the language is mostly used by people who manage
infrastructure at their organisations. The role of this group is usually
referred to as "operator", "devops" (Development and Operations) or "SRE" (Site
Reliability Engineer).

One common generalization of this group is that they are focused on maintaining
infrastructure within an organization, and often write and maintain their own
software to achieve this goal. They are adept enough at programming to achieve
their goals, but have no need or desire to be as skilled in programming as
dedicated software engineers, because their time is best spent elsewhere.

As with everything, there are exceptions to the rule, and many people in this
group _are_ highly skilled software engineers, but VRL must capture the largest
possible segment of this group, and should therefor be limited in complexity.

Therefor, when extending the feature set of VRL, design **the feature for the
intended target audience**, which will likely mean choosing different trade-offs
than you'd make if you were to design the feature for your personal needs.

## Language Limits

There are a number of features that we've so far rejected to implement in the
language. This might change in the future, but there should be a good reason to
do so.

- modules (see: [#5507][]
- classes
- user-defined functions
- `goto` statements
- network calls (see [#4517][])

[#5507]: https://github.com/timberio/vector/issues/5507
[#4517]: https://github.com/timberio/vector/issues/4517#issuecomment-754160338

## Conventions

### Functions

#### Composition

- Favor function composition over single-purpose functions.

- If a problem needs to be solved in multiple steps, consider adding
  single-purpose functions for each individual step.

- If useability or readability is hurt by composition, favor single-purpose
  functions.

#### Naming

- Function names are lower-cased (e.g. `round`).

- Multi-name functions use underscore (`_`) as a separator (e.g.
  `encode_key_value`).

- Favor explicit verbose names over terse ones (e.g.
  `parse_aws_cloudwatch_log_subscription_message` over `parse_aws_cwl_msg`).

- Use `parse_*` for string decoding functions (e.g. `parse_json` and
  `parse_grok`).

- Use `encode_*` for string encoding functions (e.g. `encode_base64`).

- Use `to_*` to convert from one type to another (e.g. `to_bool`).

- Use `format_*` for string formatting functions (e.g. `format_timestamp` and
  `format_number`).

#### Return Types

- Return boolean from `is_*` functions (e.g. `is_string`).

- Return an error when `parse_*` functions fail to decode the string.

#### Mutability

As a general rule, functions never mutate values.

Favor this:

```coffee
# explicitly assign the parsed JSON to `.message`
.message = parse_json(.message)
```

Over this:

```coffee
# mutate `.message` in place
parse_json(.message)
```

There are exceptions to this rule (such as the `del` function), but they are
limited, and additional exceptions should be well reasoned.

#### Fallibility

- A function must be marked as fallible if it can fail in any way.

- Design a function with the goal of making it infallible.

- But don't hide fallibility for the sake of persuing the previous rule.

- A function implementation can assume it receives the argument type it has
  defined.

- `parse_*` functions should almost always error when used incorrectly.

- `to_*` functions must never fail.

#### Signatures

- Functions can have zero or more parameters (e.g. `now()` and `assert(true)`).

- Argument naming follows the same conventions as function naming.

- For one or more parameters, the first parameter must be the "target" of the
  function (e.g. `parse_regex(target: <string>, pattern: <regex>)`).

- The first parameter must therefor almost always be named `target`.

- TODO

### Errors

TODO

## Patterns

The following is a list of patterns we've experienced while writing and using
VRL. This section of the document is intended to be updated frequently with new
insights.

These insights are meant to guide future design decisions, and shape our
thinking of the language as it matures and we learn more about its strengths and
weaknesses from our users.

### Error Chaining

Observability data can be structured in unexpected ways. A common pattern is to
try to decode the data in one way, only to try a different decoder if the first
one failed.

This pattern uses [function calls][] and [error coalescing][] to achieve its
goal:

```coffee
data = parse_json(.message) ??
       parse_nginx_log(.message) ?? 
       parse_apache_log(.message) ??
       { "error": "invalid data format" }
```

TODO
