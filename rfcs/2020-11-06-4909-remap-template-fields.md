# RFC 4909 - 2020-11-06 - Remap support for template strings

We would like to unify the templating syntax within configuration fields with
the Remap language.

## Scope

This RFC will look into ways we can use Remap whilst still supporting the
current method of templating fields (known throughout this RFC as the template
syntax).

## Motivation

Using the Remap language rather than the template syntax provides much greater
power in how these fields are defined.

The advantages of using Remap rather than the existing template syntax are:

- One familiar syntax and function reference for Vector.
- Access to all of remap's functions for templating.

However, we do still need to support the template syntax in order to maintain
backward compatability.

## Internal Proposal

There are two issues that need resolving to allow Remap to be used.

### Use distinct syntax to distinguish between template and Remap syntax

We will specify that double parentheses (`{{..}}`) are used for template
syntax (the current technique) and triple parentheses (`{{{...}}}`) are used
for Remap syntax.

This would provide a clear and unambiguous way to distinguish between the
syntaxes rather than rely on a set of heuristics.

The script will need to return a value that can resolve to a String. `to_string`
will be called on the final result. `String`, `Integer`, `Bool`, `Timestamp`
will be allowed results. The script will, as much as possible, resolve this at
loadtime. However, as event field types can resolve any type there will be some
limitations to how much validation can be done.

### Removing fields used in the template

A feature available with the template syntax is to be able to list all the
fields used within the template.

The Loki sink wants a list of the fields that were used in the
template. It uses this list to remove these fields from the event sent to Loki.

If you had this sink:

```toml
[sinks.loki]
  type = "loki"
  inputs = ["json"]
  endpoint = "http://localhost:3100"
  remove_label_fields = true
  labels.key = "{{foo}}-{{buzz}}"
```

And this event was sent to it:

```json
{"foo": "bar", "buzz": "zab", "message1": "thing", "message2": "thong"}
```

The actual message sent to Loki would be:

```json
{"message1": "thing", "message2": "thong"}
```

With the label `key = bar-zab` attached to the message. The fields `foo` and
`buzz` that were used in defining the label have been removed from the message.

It is not going to be possible to do this with the Remap syntax as it is much
more complicated and ambiguous to determine which fields are used to generate
the templated string.

### Add `remap.before` and `remap.after` fields

To replicate this functionality we will add `remap.before` and `remap.after`
fields to the configuration of the source. (It will be useful in time to add
these fields to every component within Vector).

The user will be able to specify any fields they want dropped from the event
in the `remap.after` script.

```toml
[sinks.loki]
  type = "loki"
  inputs = ["json"]
  endpoint = "http://localhost:3100"
  remove_label_fields = true
  labels.key = "{{{ .foo }}}-{{{ .buzz }}}"
  remap.after = """del(.foo, .buzz)"""
```

## Rationale

The benefits of using Remap in the template fields are:

- One familiar syntax and function reference for Vector.
- Access to all of remap's functions for templating.
- Less code to manage (once the old template fields are deprecated).

## Drawbacks

There could be an additional maintenance burden. The Remap syntax is more
complex which does mean there is more of a learning curve for the user and more
likelihood that they make mistakes.

## Alternatives

### Do nothing

We do already have the existing template syntax. Perhaps we can stick with this.

The advantage of using Remap for these fields are that it allows more
flexibility in defining how the event is used. However, given that remap can be
used as a transform, should the user really need this they could put a Remap
transform in the process to process these fields so they can be easily used in
the template for the next phase.

### Detect which syntax is being used - template or remap

Rather than specifying different syntax to identify if a script is Remap or
template, we could allow `{{..}}` for both languages and autodetect the sytax
based on a number of heuristics.

- With the template syntax each templated field will contain a single path - for
  example `{{ message.field[2] }}`.

- With the Remap syntax it is possible to contain a single path, however this
  will be prefixed by a `.` - eg. `{{ .message.field[2] }}`. Any more complex
  syntax will be at minimum an `if` statement -
  eg. `{{ if .bar { .baz } else { .boo } }}`
  or a function call - eg. `{{ replace(.bar, "foo", "bar) }}`

So, we can assume that if (after trimming) the template within the `{{..}}`
starts with a `.` or it contains a single bracket `{` or parentheses (`(..)`)
the syntax will be Remap syntax and should be parsed as such.

### Returning fields at load time

To allow the `remove_label_fields` option to still work with Remap scripts,
Remap could keep track of the fields used whilst running the script.

There are several options:

- *Keep track of all fields used in the script.*
This script, `if .foo { .bar } else { .baz }`, would result in all three
fields being returned - `.foo`, `.bar` and `.baz`, and subsequently removed
from the message sent to Loki.

- *Keep track of the fields used in the run path.*
The script, `if .foo { .bar } else { .baz }`,
would result in `.foo` and either `.bar` or `.baz` being returned.

If necessary, Remap could distinguish between fields that are used in the
condition and those used it the result, so only `.bar` or `.baz` could be
returned.

There are likely to be a number of edge cases that would need to be thought
through if we took this approach. For example, if a field is used in a function
but it's value is only used to influence the result - should it be included?

```coffeescript
replace(.field1, .field2, .field3)
```

The result is the value of `field1`, but with any occurrence of the value of
`field2` being replaced by `field3`. Which fields would be correct to include
in this list?

### Create a `mark` function that remap can use to drop fields after use

To replicate this functionality a remap function can be added to mark fields
for deletion, call it `mark(field)` for now. The above script could be written
in the remap syntax as:

```coffeescript
  labels.key = """
  {{
    mark(.foo)
    mark(.buzz)
    .foo + "-" + .buzz
  }}
  """
```

The function `mark` is technically not a mutable function, so it is safe to use
in template fields. The event is kept intact throughout the process and the
fields are only removed from the event at the end. This differs from the `del`
function in that `del` will remove the field immediately and it would not be
available for use after that point. The order in which the template fields are
calculated would have an impact on the final result.

## Plan Of Attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ] Update the template logic to detect the `{{{..}}}` syntax and call Remap
      to resolve the values.
- [ ] Implement `remap.before` and `remap.after` as functions that can be called
      by any component.
- [ ] Call `remap.before` and `remap.after` in the `Loki` sink.
- [ ] Call `remap.before` and `remap.after` in any sink that uses template
    fields.
