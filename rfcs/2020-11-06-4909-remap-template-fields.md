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

### Determine which syntax is being used

We need to determine if a template field is using Remap syntax or the 
template syntax. 

- With the template syntax each templated field will contain a single path - for
example `{{ message.field[2] }}`.

- With the Remap syntax it is possible to contain a single path, however this will
be prefixed by a `.` - eg. `{{ .message.field[2] }}`. Any more complex syntax
will be at minimum an `if` statement - eg. `{{ if .bar { .baz } else { .boo } }}`
or a function call - eg. `{{ replace(.bar, "foo", "bar) }}`

So, we can assume that if (after trimming) the template within the `{{..}}`
starts with a `.` or it contains a single bracket `{` or parentheses (`(..)`)
the syntax will be Remap syntax and should be parsed as such.


### Removing fields used in the template

A feature available with the template syntax is to be able to list all the fields
used within the template.

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

```
{"message1": "thing", "message2": "thong"}
```

With the label `key = bar-zab` attached to the message. The fields `foo` and
`buzz` that were used in defining the label have been removed from the message.

It is not going to be possible to do this with the Remap syntax as it is much
more complicated and ambiguous to determine which fields are used to generate
the templated string. To replicate this functionality a remap function will be
needed to mark fields for deletion, call it `mark(field)` for now. The above
script could be written in the remap syntax as:

```
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


## Rationale

The benefits of using Remap in the template fields are:

 - One familiar syntax and function reference for Vector.
 - Access to all of remap's functions for templating.
 - Less code to manage (once the old template fields are deprecated).


## Drawbacks

There could be an additional maintenance burden. The Remap syntax is more
complex which does mean there is more of a learning curve for the user and more
likelyhood that they make mistakes.


## Alternatives

### Do nothing

We do already have the existing template syntax. Perhaps we can stick with this.

The advantage of using Remap for these fields are that it allows more 
flexibility in defining how the event is used. However, given that remap can be 
used as a transform, should the user really need this they could put a Remap 
transform in the process to process these fields so they can be easily used in 
the template for the next phase.

### Use distinct syntax to distinguish between template and Remap syntax.

We could specify that double parentheses (`{{..}}`) are used for template
syntax (the current technique) and triple parentheses (`{{{...}}}`) are used 
for Remap syntax.

This would provide a clear and unambiguous way to distinguish between the
syntaxes rather than rely on a set of heuristics.

### Returning fields at load time

Instead of requiring the user to mark the fields that are removed, Remap could
keep track of the fields used whilst running the script.

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

```
replace(.field1, .field2, .field3)
```

The result is the value of `field1`, but with any occurrence of the value of 
`field2` being replaced by `field3`. Which fields would be correct to include
in this list?


## Outstanding Questions

- How important is it for the Loki sink to remove fields used in the template
  from the event?

## Plan Of Attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ] Submit a PR with spike-level code _roughly_ demonstrating the change.
- [ ] Incremental change #1
- [ ] Incremental change #2
- [ ] ...

Note: This can be filled out during the review process.
