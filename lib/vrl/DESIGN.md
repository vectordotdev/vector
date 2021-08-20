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
  * [Syntax](#syntax)
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

* **Safety and performance over ease of use.** VRL programs facilitate data
  processing for mission critical production systems. They are run millions of
  times per day for large Vector users and usually written once. Therefore,
  safety and performance are prioritized over developer ease of use.

* **The best VRL program is the one that most clearly expresses its output.**
  VRL is an expression-oriented DSL designed to express data transformations. It
  is not a programming language. Users should not sacrifice the clarity of their
  data transformation for things like performance. The best VRL program is the
  one that most clearly describes the intended output. There should be no
  "tricks" to making VRL fast that takeaway from readability.

## Why VRL

VRL exists to solve the need for a _safe_ and _performant_ **domain specific
language** (DSL) to remap observability data.

Its purpose is to hit a sweet spot between Turing complete languages such as
Lua, and static transforms such as Vector's old `rename_fields`. It should be
flexible enough to cover most remapping use-cases, without requiring the
flexibility and downsides of a full programming language, such as poor
readability and performance.

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
possible segment of this group, and should therefore be limited in complexity.

Therefore, when extending the feature set of VRL, design **the feature for the
intended target audience**, which will likely mean choosing different trade-offs
than you'd make if you were to design the feature for your personal needs.

## Language Limits

There are a number of features that we've so far rejected to implement in the
language. This might change in the future, but there should be a good reason to
do so.

- modules (see: [#5507][])

  So far, we've had no need to split up functions over multiple modules. The
  function naming rules make it so that most functions are already grouped by
  their logical usage patterns.

- classes

  Given that VRL is a simple DSL, and that any indirection in a program's source
  can lead to confusion, we've decided against introducing the concept of
  classes, and instead focused on the usage of function calls to solve operator
  needs.

- user-defined functions

  User-defined functions again produce indirection. While it _might_ be useful
  to some extremely large use-cases, in most cases, allowing people to read
  a program from top to bottom without having to jump around is more clear in
  the context within which VRL is used (remapping).

- network calls (see [#4517][] and [#8717][])

  In order to avoid performance footguns, we want to ensure each function is as
  performant as it can be, and there's no way to use functions in such a way
  that performance of a VRL program tanks. We might introduce network calls at
  some point, if we find a good caching solution to solve most of our concerns,
  but so far we've avoided any network calls inside our stdlib.

[#5507]: https://github.com/timberio/vector/issues/5507
[#4517]: https://github.com/timberio/vector/issues/4517#issuecomment-754160338
[#8717]: https://github.com/timberio/vector/pull/8717

## Conventions

### Syntax

Keep VRL as syntax-light as possible. The less symbols, the more readable a VRL
program is.

Use functions whenever possible, and only introduce new syntax if a common
pattern warrants a more convenient syntax-based solution, or functions are too
limited in their capability to solve the required need.

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

- Functions should be preceded with their function category for organization and
  discovery.

  - Use `parse_*` for string to type decoding functions (e.g. `parse_json` and
    `parse_grok`).
  
  - Use `decode_*` for string to string decoding functions (e.g.
    `decode_base64`).
  
  - Use `encode_*` for string encoding functions (e.g. `encode_base64`).
  
  - Use `to_*` to convert from one type to another (e.g. `to_bool`).

  - Use `is_*` to determine if the provided value is of a given type (e.g.
    `is_string` or `is_json`).
  
  - Use `format_*` for string formatting functions (e.g. `format_timestamp` and
    `format_number`).

#### Return Types

- Return boolean from `is_*` functions (e.g. `is_string`).

- Return a string from `encode_*` functions (e.g. `encode_base64`).

- Return an error whenever the function can fail at runtime.

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

- For one or more parameters, the first parameter must be the "value" the
  function is acting on (e.g. `parse_regex(value: <string>, pattern: <regex>)`).

- The first parameter must therefore almost always be named `value`.

- The exception to this is when you're dealing with an actual VRL path (e.g.
  `del(path)`) or in special cases such as `assert`.

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
