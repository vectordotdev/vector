---
title: VRL error reference
short: Errors
weight: 2
---

VRL is a [fail-safe][fail_safety] language, which means that a VRL program doesn't compile unless
every potential error is handled. Observability data is notoriously unpredictable and fail safety
ensures that your VRL programs elegantly handle malformed data.

## Compile-time errors

{{< vrl/errors/compile-time >}}

## Runtime errors

A runtime error occurs after compilation and during program runtime. Because VRL is fail safe, all
runtime errors must be [handled](#handling). This forces you to address how VRL programs should
respond to errors.

Runtime errors are strings that describe the error.

### Handling

You have three options for handling errors in VRL:

* [Assign](#assigning) the error
* [Coalesce](#coalescing) the error
* [Raise](#raising) the error

#### Assigning

As documented in the [assignment expression reference][assign], you can **assign** errors when
invoking an expression that's fallible. When assigned, runtime errors are simple strings:

```coffee
structured, err = parse_json("not json")
if err != null {
  log("Unable to parse JSON: " + err, level: "error")
} else {
  . = merge(., structured)
}
```

If the expression fails, the `ok` assignment target is assigned the "empty" value of its type:

```coffee
# `.foo` can be `100` or `"not an int"`
foo, err = to_int(.foo)

# `err` can be `null` or `"unable to coerce value to integer"`
if err == null {
  # `foo` can be `100` or `0`
  .result = foo * 5
}
```

The above example compiles because `foo` will either be assigned the integer representation of
`.foo` if it can be coerced to an integer, or it will be set to the "empty integer value" `0` if
`.foo` can't be coerced into an integer.

Because of this, it is important to always check whether `err` is null before using the `ok` value
of an infallible assignment.

##### Empty values

Type | Empty value
:----|:-----------
String | `""`
Integer | `0`
Float | `0.0`
Boolean | `false`
Object | `{}`
Array | `[]`
Timestamp | `t'1970-01-01T00:00:00Z'` (Unix epoch)
Regular expression | `r''`
Null | `null`

#### Coalescing

As documented in the [coalesce expression reference][coalesce], you can **coalesce** errors to
efficiently step through multiple expressions:

```coffee
structured = parse_json("not json") ?? parse_syslog("not syslog") ?? {}
. = merge(., structured)
```

#### Raising

As documented in the [function call reference][call], you can **raise** errors to immediately abort
the program by adding a `!` to the end of the function name:

```coffee
structured = parse_json!("not json")
. = merge(., structured)
```

{{< warning title="Raising errors should be used with caution" >}}
While raising errors can simplify your program, you should think carefully before aborting your
program. If this operation is critical to the structure of your data you should abort, otherwise
consider handling the error and proceeding with the rest of your program.
{{< /warning >}}

[assign]: /docs/reference/vrl/expressions/#assignment
[call]: /docs/reference/vrl/expressions/#function-call
[coalesce]: /docs/reference/vrl/expressions/#coalesce
[fail_safety]: /docs/reference/vrl/#fail-safety
