---
title: VRL expression reference
short: Expressions
weight: 2
---

## Syntax

VRL programs can be constructed with the following syntax rules.

### Comment

A *comment* serves as program documentation and is idenfied with `#`. Each line must be preceded by a `#` character. VRL curren't doesn't allow for block comments.

#### Examples

```ruby
# comment
```

```ruby
# multi-line
# comment
```

### Keywords

Keywords are reserved words that are used for primitive language features, such as `if`, and can't be used as variable assignments or other custom directives. The following words are reserved (in alphabetical order):

* `abort`
* `as`
* `break`
* `continue`
* `else`
* `false`
* `for`
* `if`
* `impl`
* `in`
* `let`
* `loop`
* `null`
* `return`
* `self`
* `std`
* `then`
* `this`
* `true`
* `type`
* `until`
* `use`
* `while`

## Literal expressions

As in most other languages, **literals** in VRL are values written exactly as they are meant to be interpreted. Literals include things like strings, Booleans, and integers.

### Array

An **array** literal is a comma-delimited set of expressions that represents a contiguous growable array type.

#### Examples

* `[]`
* `["first", "second", "third"]`
* `["mixed", 1, 1.0, true, false, {"foo": "bar"}]`
* `["first-level", ["second-level", ["third-level"]]`
* `[.field1, .field2, to_int!("2"), variable_1]`
* `[ "expressions", 1 + 2, 2 == 5, true || false ]`

### Boolean

A **Boolean** literal represents a binary value that can be either `true` or `false`.

#### Examples

* `true`
* `false`

### Float

A **float** literal is a decimal representation of a 64-bit floating-point type (specifically, the “binary64” type defined in IEEE 754-2008).

A decimal floating-point literal consists of an integer part (decimal digits), a decimal point, a fractional part (decimal digits).

#### Examples

* `1_000_000.01`
* `1000000.01`
* `1.001`

### Integer

An **integer** literal is a sequence of digits representing a 64-bit signed integer type.

{{< info title="Limits" >}}
Integers in VRL can range from -9223372036854775807 to 9223372036854775807. Integers outside that range are [wrapped].

[wrapped]: https://en.wikipedia.org/wiki/Integer_overflow
{{< /info >}}

{{< info title="Underscores" >}}
Integers can use underscore (`_`) characters instead of `,` to make them human readable. Some examples:

* `1_000_000`
* `9_876`
* `123_456_789`
{{< /info >}}

#### Examples

* `1_000_000`
* `1000000`

### Null

A **null** literal is the absence of a defined value.

#### Examples

* `null`

### Object

An **object** literal is a growable key/value structure that is syntactically equivalent to a JSON object.

A well-formed JSON document is a valid VRL object.

{{< info title="Ordering" >}}
Object fields are ordered by key alphabetically and ascending. Operations like encoding into JSON thus produce a string with keys in ascending alphabetical order.
{{< /info >}}

#### Examples

```json
{ "field1": "value1", "field2": [ "value2", "value3", "value4" ], "field3": { "field4": "value5" } }
```

```json
{ "field1": .some_path, "field2": some_variable, "field3": { "subfield": "some value" } }
```

### Regular expression

A **regular expression** literal represents a [Regular Expression][regex] used for string matching and parsing.

Regular expressions are defined by the r sigil and wrapped with single quotes (`r'...'`). The value between the quotes uses the [Rust regex syntax][rust_regex].

{{< info title="Flags" >}}
Regular expressions allow for flags. Flags can be combined, as in `r'(?ixm)pattern'`, `r'(?im)pattern'`, etc.

To learn more about regular expressions in Rust—and by extension in VRL—we strongly recommend the in-browser [Rustexp expression editor and tester][rustexp].

[rustexp]: https://rustexp.lpil.uk
{{< /info >}}

{{< info title="Named captures" >}}
Regular expressions support named capture groups, allowing extractions to be associated with keys. Named captures should be preceded with a `?P<name>` declaraction. This regex, for example...

```ruby
r'(?P<y>\d{4})-(?P<m>\d{2})-(?P<d>\d{2})'
```

...extracts captures with the `y`, `m`, and `d` keys.
{{< /info >}}

#### Examples

* `r'^Hello, World!$'`
* `r'^Hello, World!$'i`
* `r'^\d{4}-\d{2}-\d{2}$'`
* `r'(?P<y>\d{4})-(?P<m>\d{2})-(?P<d>\d{2})'`

### String

A **string** literal is a [UTF-8–encoded][utf8] string. String literals can be raw or interpreted.

**Raw string** literals are composed of the uninterpreted (implicitly UTF-8-encoded) characters between single quotes identified with the s sigil and wrapped with single quotes (`s'...'`); in particular, backslashes have no special meaning and the string may contain newlines.

**Interpreted** string literals are character sequences between double quotes (`"..."`). Within the quotes, any character may appear except newline and unescaped double quote. The text between the quotes forms the result of the literal, with backslash escapes interpreted as defined below.

{{< info title="Backslash escapes" >}}
Special characters, such as newlines, can be expressed with a backslash escape.
{{< /info >}}

{{< info title="Concatenation" >}}
Strings can be concatenated with the `+` operator.
{{< /info >}}

{{< info title="Invalid characters" >}}
Invalid UTF-8 sequences are replaced with the `�` character.
{{< /info >}}

#### Examples

* `"Hello, world! 🌎"`
* `"Hello, world! \\u1F30E"`
* `s'Hello, world!'`
* `s'{ "foo": "bar" }'`

### Timestamp

A **timestamp** literal defines a native timestamp expressed in the [RFC 3339 format][rfc3339] with a nanosecond precision.

Timestamp literals are defined by the `t` sigil and wrapped with single quotes (`t'2021-02-11T10:32:50.553955473Z'`).

{{< info title="Timezones" >}}
As defined in [RFC 3339][rfc_3339] format, timestamp literals support UTC and local offsets.

[rfc_3339]: https://tools.ietf.org/html/rfc3339
{{< /info >}}

#### Examples

* `t'2021-02-11T10:32:50.553955473Z'`
* `t'2021-02-11T10:32:50.553Z'`
* `t'2021-02-11T10:32:50.553-04:00'`

## Dynamic expressions

VRL is an expression-oriented language. A VRL program consists entirely of expressions and every expression returns a value.

{{< vrl/data "expressions" >}}

[regex]: https://en.wikipedia.org/wiki/Regular_expression
[rfc3339]: https://tools.ietf.org/html/rfc3339
[rust_regex]: https://docs.rs/regex/latest/regex/#syntax
[utf8]: https://en.wikipedia.org/wiki/UTF-8
