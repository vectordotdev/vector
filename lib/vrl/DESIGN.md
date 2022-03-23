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
  * [Performance](#performance)
    * [Copying Data](#copying-data)
    * [Program Optimizations](#program-optimizations)
  * [Diagnostics](#diagnostics)
  * [Syntax](#syntax)
  * [Fallibility](#fallibility)
    * [Fallible Expressions](#fallible-expressions)
    * [Type Checking](#type-checking)
    * [Progressive Type Checking](#progressive-type-checking)
  * [Errors](#errors)
  * [Functions](#functions)
    * [Composition](#composition)
    * [Naming](#naming)
    * [Return Types](#return-types)
    * [Mutability](#mutability)
    * [Fallibility](#fallibility-1)
    * [Signatures](#signatures)
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
  "tricks" to making VRL fast that take away from readability.

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

* modules (see: [#5507][])

  So far, we've had no need to split up functions over multiple modules. The
  function naming rules make it so that most functions are already grouped by
  their logical usage patterns.

* classes

  Given that VRL is a simple DSL, and that any indirection in a program's source
  can lead to confusion, we've decided against introducing the concept of
  classes, and instead focused on the usage of function calls to solve operator
  needs.

* user-defined functions

  User-defined functions again produce indirection. While it _might_ be useful
  to some extremely large use-cases, in most cases, allowing people to read
  a program from top to bottom without having to jump around is more clear in
  the context within which VRL is used.

* network calls (see [#4517][] and [#8717][])

  In order to avoid performance foot guns, we want to ensure each function is as
  performant as it can be, and there's no way to use functions in such a way
  that performance of a VRL program tanks. We might introduce network calls at
  some point, if we find a good caching solution to solve most of our concerns,
  but so far we've avoided any network calls inside our stdlib.

* assignable closures (see [#9001])

  While we _do_ support closures, they are tied to function calls, and cannot be
  used elsewhere. This also means closures cannot be assigned to variables, and
  re-used between function-calls. This decision was made because it can lead to
  poor-performing code (to the extend of introducing infinite loops), and makes
  code less clear to reason about. While this is undoubtedly a powerful feature
  to have, the cons do not outweigh the pro's.

[#5507]: https://github.com/vectordotdev/vector/issues/5507
[#4517]: https://github.com/vectordotdev/vector/issues/4517#issuecomment-754160338
[#8717]: https://github.com/vectordotdev/vector/pull/8717
[#9001]: https://github.com/vectordotdev/vector/pull/9001#discussion_r701830595

## Conventions

### Performance

The performance of VRL is a corner-stone of the language. However, given the
target audience, and the goal to make the language as simple and
straight-forward as possible, there are always trade-offs to be made when
considering the performance implications of a feature or design decision.

#### Copying Data

For example, VRL is an expression-oriented language, and we favor returning
a new copy of a piece of manipulated data over mutating the underlying data.

That is, you write this:

```coffee
# explicitly assign the parsed JSON to `.message`
.message = parse_json(.message)
```

Instead of this:

```coffee
# mutate `.message` in place
parse_json(.message)
```

While this results in less performance, it makes VRL programs easier to reason
about, which _in this particular case_ weigh more heavily than the performance
implications.

#### Program Optimizations

We plan on introducing an "optimization step" to the compiler in the future,
that would allow us to rewrite parts of a program into a more optimized variant,
without having the operator having to worry about these transformations.

This means that if there's a reasonable path forward towards optimizing certain
language constructs internally, we should not burden the operator with
using/applying to optimizations manually.

For example, we favor composition over single-purpose functions, and while each
individual function call adds extra performance overhead, the thought is that in
the future, we can optimize multiple function calls into a single call
(inlining), without having to expose this optimization technique to operators.

### Diagnostics

Diagnostic messages shown by the compiler during compilation are one of the
biggest tools we expose to operators to help them write correct programs.

Adding diagnostic messages to new and existing features in VRL should never be
an afterthought.

### Syntax

Keep VRL as syntax-light as possible. The less symbols, the more readable a VRL
program is.

Use functions whenever possible, and only introduce new syntax if a common
pattern warrants a more convenient syntax-based solution, or functions are too
limited in their capability to solve the required need.

### Fallibility

A VRL program must be **infallible by default** once the compiler generates
a program from the provided source. This means that any expression that can
result in an error (adding a string and an object, dividing by zero, calling
a fallible function) must be explicitly handled before the compiler accepts the
input source.

The fallibility system is an important part of the language and its goals, and
is also the part that most often trips people up. This chapter tries to shed
some light on its inner workings.

#### Fallible Expressions

When the VRL compiler compiles a source to a valid program, it queries each
expression whether the expression itself can fail at runtime or not. If it can,
the compiler refuses to compile the program, until the operator handles the
failure case.

For example:

```coffee
. = parse_json(.message)
```

The above program can fail at runtime, because there's no guarantee the
`message` field contains a JSON-encoded string.

The operator needs to handle the failure case using one of the available
[failure-handling features][fail] in VRL.

#### Type Checking

In addition to expressions being fallible, the type checker also considers
a program fallible if the type expected by an expression cannot be guaranteed at
compile-time.

For example:

```coffee
.message = "log message: " + .log
```

In this case, the `log` field type cannot be determined at compile-time, and
thus concatenating the field value with a string might fail (e.g. if it's an
array, or any other type that cannot be combined with a string).

This too needs to be handled at compile-time by the operator.

#### Progressive Type Checking

A quirk in the type checker is that arguments passed to functions are _not_ type
checked individually. Instead, the function call itself can be marked fallible
if any of its arguments do not adhere to the expected type.

For example:

```coffee
upcase(.message)
```

Upcasing a string is an infallible operation, however, because we can't
guarantee that the `message` field will actually be a string, the function call
is still invalid, as the function is marked as fallible.

This decision [was made][#6507] for ergonomics purposes.

[fail]: https://vrl.dev/errors/#runtime-errors
[#6507]: https://github.com/vectordotdev/vector/issues/6507

### Errors

* All errors are caught at compile-time by the compiler (see "fallibility"
  chapter).

* The only exception to this rule is if the operator explicitly allows
  a function to fail the program at runtime (e.g. `safe_call()` vs
  `unsafe_call!()`).

* A function should be marked as "fallible" if its internal implementation can
  fail.

* A function _should not_ be marked as fallible if it receives the wrong
  argument type. This is handled by the compiler.

* Errors should contain explicit messages detailing what went wrong, and how the
  operator can solve the problem.

### Functions

#### Composition

* Favor function composition over single-purpose functions.

* If a problem needs to be solved in multiple steps, consider adding
  single-purpose functions for each individual step.

* If usability or readability is hurt by composition, favor single-purpose
  functions.

#### Naming

* Function names are lower-cased (e.g. `round`).

* Multi-name functions use underscore (`_`) as a separator (e.g.
  `encode_key_value`).

* Favor explicit verbose names over terse ones (e.g.
  `parse_aws_cloudwatch_log_subscription_message` over `parse_aws_cwl_msg`).

* Functions should be preceded with their function category for organization and
  discovery.

  * Use `parse_*` for functions that decode a string to another type of data
    (e.g. `parse_json` and `parse_grok`).

  * Use `decode_*` for string to string decoding functions (e.g.
    `decode_base64`).

  * Use `encode_*` for string encoding functions (e.g. `encode_base64`).

  * Use `to_*` to convert from one type to another (e.g. `to_bool`).

  * Use `is_*` to determine if the provided value is of a given type (e.g.
    `is_string` or `is_json`).

  * Use `format_*` for string formatting functions (e.g. `format_timestamp` and
    `format_number`).

  * Use `get_*` for functions that return a single result, or error if zero or
    more results are found.

  * Use `find_*` when multiple possible results are returned in an array.

#### Return Types

* Return boolean from `is_*` functions (e.g. `is_string`).

* Return a string from `encode_*` functions (e.g. `encode_base64`).

* Return an error whenever the function can fail at runtime.

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

* A function must be marked as fallible if it can fail in any way.

* A function should be designed with the goal of making it infallible.

* A function must not hide fallibility for the sake of pursuing the previous
  rule.

* A function implementation may assume it receives the argument type it has
  defined.

* `parse_*` functions should almost always error when used incorrectly.

* `get_*` functions should fail when it can't find a single result to return.

* `to_*` functions must never fail.

#### Signatures

* Functions can have zero or more parameters (e.g. `now()` and `assert(true)`).

* Argument naming follows the same conventions as function naming.

* For one or more parameters, the first parameter must be the "value" the
  function is acting on (e.g. `parse_regex(value: <string>, pattern: <regex>)`).

* The first parameter must therefore almost always be named `value`.

* The exception to this is when you're dealing with an actual VRL path (e.g.
  `del(path)`) or in special cases such as `assert`.

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
