# RFC 3736 - 2020-10-29 - Ability to chain parsing functions in the remap syntax

Vector needs to be able to parse a variety of log formats in different ways.

This RFC proposes enhancements to the Remap language to give it the ability to
chain multiple parsing strategies in the language. For example, a number of
regular expressions can be chained together and each run sequentially until one
passes.


## Scope

This RFC will focus on the syntax needed to enhance the Remap language to allow
for multiple branches as well as the functionality required to allow the scripts
to most efficiently run within these branches.

## Motivation

The best way to achieve this currently is with the `swimlanes` transform.

However, the complexity required to efficiently maintain multiple parsing
strategies using swimlanes is fairly complex which
can lead to error prone and inefficient transforms.

See issues:

[#2418](https://github.com/vectordotdev/vector/issues/2418)
[#1477](https://github.com/vectordotdev/vector/issues/1477)

## Doc-level Proposal

Within a `remap` mapping it is possible to specify a number of conditions. Each
condition will be run sequentially - the first condition that passes the
associated mapping block will be performed. A condition will pass when it
evaluates to a boolean `True`.

This is performed using `if...else if...else`:

```rust
if ... {
   ...
} else if ... {
   ...
} else if ... {
   ...
} else {
   ...
}
```

The mapping will start at the condition specified by the first `if` and will
proceed along each condition specified by each `else if` sequentially until
it finds one that passes. The ensuing code block will be executed.

A default code block can be specified at the end using `else`. This is run if
none of the conditions pass. If there is no `else` and no condition matches, no
code blocks are run.


## Condition

It is possible to have multiple statements within the condition. The statements
must be surrounded with parentheses and separated by either a semicolon or a
new line.

```rust
(statement1; statement2; statement3)
```

or

```rust
(statement1
 statement2
 statement3)
```

This allows you to, for example, assign the results of a regular expression
to a variable and then test the results. If the regular expression matches
you have the results to work with.

The following code will be possible:

```coffee
if ($match = matches(.message, /^Started (?P<method>[^\s]*) for (?P<remote_addr>[^\s]*)/)
    !is_empty($match)) {
  .method = $match.method
  .remote_addr = $match.remote_addr
  .source = "nginx"
} else if ($match = matches(.message, /^(?P<remote_addr>[^\s]*).*"(?P<method>[^\s]*).*"$/)
           !is_empty($match)) {
  .method = $match.method
  .remote_addr = $match.remote_addr
  .source = "haproxy"
} else {
  .source = "none"
}
```

The final statement must evaluate to a Boolean.

If there is only a single predicate evaluated in the condition, the parentheses
are not required.

Assigning values to variables is only permitted if the condition is wrapped
with parentheses. If the condition is not wrapped in a group, only the
double equals (`==`) is permitted. This helps to avoid the potential bug
where a typo with a single equals results in valid, but incorrect code.

The scoping for any assignment to variables in the condition is global within
the script. The variable is modified even if the condition fails.

*Note, `matches` is currently unimplemented, but in this example it is
intended as a function that would match a regular expression and return any
matching groups in a `Value::map`. If there were no match, it returns an
empty map.*

## Rationale

If this functionality can be encapsulated within the Remap language it will
allow users to simplify their configuration. All processing involved would be
contained within a single transform that can be precisely designed to do what
the user needs in the minimum number of steps.

## Prior Art

Currently, we have the [Swimlanes](https://vector.dev/docs/reference/transforms/swimlanes/)
transform. Swimlanes can route the event to a transform or sink
according to a provided condition. These transforms could be other remap
transforms that can do the required processing for that condition.

Whilst this is possible, it does complicate the user's configuration as they
will need multiple transforms specified. With this RFC in place, only one
transform will likely be necessary to do the processing required.


## Drawbacks

Because the scope of the variables is global within the script, by allowing side
effects within the condition, the user could potentially write code that they
didn't intend. For example, The assignment still occurs even if the condition
doesn't succeed. The script writer needs to be aware that the variable will not
still hold the original value if the predicate doesn't succeed.


## Alternatives

### Don't enforce boolean conditions

We could loosen the typing rules and allow non boolean values to signal a
success of a condition. This would mean we wouldn't need to have multiple
statements within a condition. In the following code, match returns a non-empty
map on success, which is then passed to the condition after the assignment.

```coffeescript
if $match = match(.message, /^Started (?P<method>[^\s]*) for (?P<remote_addr>[^\s]*)/) {
  .method = $match.method
  .remote_addr = $match.remote_addr
  .source = "nginx"
}
```

### Use `switch`

There are some alternative syntaxes that may be worth considering. One
possibility that is common in many languages is a `switch` statement.

```coffeescript
switch {
 ($match = matches(.message, /^Started (?P<method>[^\s]*) for (?P<remote_addr>[^\s]*)/)): {
    .method = $match.method
    .remote_addr = $match.remote_addr
    .source = "nginx"
  }
 ($match = matches(.message, /^(?P<remote_addr>[^\s]*).*"(?P<method>[^\s]*).*"$/): {
    .method = $match.method
    .remote_addr = $match.remote_addr
    .source = "haproxy"
  }
  default: {
    .source = "other"
  }
}
```

The disadvantage of this approach is it is an extra syntactical element that
needs to be added to the language that would require maintenance and make the
language harder to learn. The advantage, however, is that we can then
potentially avoid having to enable `if` statements to work with non-boolean
values. However, I'm not convinced this would be a good idea as having
conditions working differently depending on their context could be confusing for
the user.

### Scoping

We could introduce scoping rules into the condition. Any modifications to the
variables would only be visible in the block that is run should the condition
succeed. This could avoid surprises outlined in the Drawbacks section where
the state is modified even on failure.

## Outstanding Questions

- One thing that the `swimlanes` can do that this RFC hasn't catered for is that
`swimlanes` can send the event to different sinks depending on a condition. With
Remap there is only one possible component, another transform or a sink, that
can accept the output of the mapping. Is this something that should be catered
for by the Remap language and this RFC?


## Plan Of Attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ] Add `else if` to the remap language parser and add multiple branches to
      the `IfStatement` struct to handle multiple conditions.
      (Already in progress, see [#4814](https://github.com/vectordotdev/vector/pull/4814))
- [ ] Enhance the parser to allow for multiple statements in the condition.
- [ ] Adapt the Evaluation code to run with multiple statements. (It is very
      possible very little work will be needed to do this.)
- [ ] Write the `matches` function that will run a regular expression and return
      any matched groups found. This is potentially out of scope for this rfc,
      but would be useful nevertheless.
