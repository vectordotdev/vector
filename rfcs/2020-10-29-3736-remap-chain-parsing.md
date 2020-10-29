# RFC 3736 - 2020-10-29 - Ability to chain parsing functions in the remap syntax

Vector needs to be able to parse a variety of log formats in different ways. 

This RFC proposes enhancements to the Remap language to give it the ability to chain multiple parsing strategies 
in the language. For example, a number of regular expressions can be chained together 
and each run sequentially until one passes.


## Scope

This RFC will focus on the syntax needed to enhance the Remap language to allow for multiple branches as well as the 
functionality required to allow the scripts to most efficiently run within these branches.

## Motivation

The best way to achieve this currently is with the `swimlanes` transform.

However, the complexity required to efficiently maintain multiple parsing strategies using swimlanes is fairly complex which
can lead to error prone and inefficient transforms.

See issues:

[#2418](https://github.com/timberio/vector/issues/2418)
[#1477](https://github.com/timberio/vector/issues/1477)

## Doc-level Proposal

Within a `remap` mapping it is possible to specify a number of conditions. Each condition will be run sequentially - the first condition 
that passes the associated mapping block will be performed. This is performed using `if...else if...else`:

```
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

The mapping will start at the condition specified by the first `if` and will proceed along each condition specified by each `else if` sequentially until
it finds one that passes. The ensuing code block will be executed.

A default code block can be specified at the end using `else`. This is run if none of the conditions pass. If there is no `else` and no condition matches,
no code blocks are run.

The script is able to make the most of any processing that has been done to run the condition - for example, if a regular expression
has been run, the block that is subsequently run can have access to the results of this regex. The Remap syntax also allows assignment to variables within 
the condition. A condition is considered to be successful if the value it returns is not `false` or `nil`. This allows a regular expression match to pass 
the condition and pass it's results into the block via the variable assignment.

The following code will be possible:

```coffee
if $match = matches(.message, /^Started (?P<method>[^\s]*) for (?P<remote_addr>[^\s]*)/) {
  .method = $match.method
  .remote_addr = $match.remote_addr
  .source = "nginx"
} else if $match = matches(.message, /^(?P<remote_addr>[^\s]*).*"(?P<method>[^\s]*).*"$/ {
  .method = $match.method
  .remote_addr = $match.remote_addr
  .source = "haproxy"
} else {
  .source = "none"
}
```

*Note, `matches` is currently unimplemented, but in this example it is intended as a function that would match a regular expression and return any matching
groups in a `Value::map`. If there were no match, it would return either `false` or `null`.* 



## Rationale

If this functionality can be encapsulated within the Remap language it will allow users to simplify their configuration. All processing
involved would be contained within a single transform that can be precisely designed to do what the user needs in the minimum number
of steps.

## Prior Art

Currently, we have the [Swimlanes](https://vector.dev/docs/reference/transforms/swimlanes/) transform. Swimlanes can route the event to a transform or sink
according to a provided condition. These transforms could be other remap transforms that can do the required processing for that condition.

Whilst this is possible, it does complicate the user's configuration as they will need multiple transforms specified. With this RFC in place, only
one transform will likely be necessary to do the processing required.


## Drawbacks

Allowing `if` statements to work with non-boolean values is a controversial topic in programming language design. It is possible to create
unintended bugs because the code hasn't been explicit enough about what should and should not pass the condition. Decisions need to be 
made as to what actually constitutes a failure condition - for example, should a result of `0` be a pass or a fail? This does add additional
documentation requirements and cognitive load when writing the script.

The same can be said for allowing assignment within an `if` statement. The condition will be valid for both a single (`=`) and double (`==`) equals
in the condition, yet both variations do very different things:

```
if .foo == true {

}
```

```
if .foo = true {

}
```

This has haunted C programmers for decades.

Also, the assignment still occurs even if the condition doesn't succeed. Side effects such as this can be unexpected for the script writer
and could cause unexpected bugs to creep in.


## Alternatives

### Enforce boolean conditions
Allowing for non-boolean conditions in the `if` condition is not strictly necessary. A similar result could be achieved by running the regular
expression twice, one with `match` which returns a boolean if it matches, and then again with `matches` which would extract the matches out of 
the text.

```coffeescript
if match(.message, /^Started (?P<method>[^\s]*) for (?P<remote_addr>[^\s]*)/) {
  $match = matches(.message, /^Started (?P<method>[^\s]*) for (?P<remote_addr>[^\s]*)/)
  .method = $match.method
  .remote_addr = $match.remote_addr
  .source = "nginx"
}
```

However, this has performance implications as the regular expression has to be run twice.

### Use `switch`
There are some alternative syntaxes that may be worth considering. One possibility that is common in many languages is a `switch` statement.

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

The disadvantage of this approach is it is an extra syntactical element that needs to be added to the language that would require maintenance and
make the language harder to learn. The advantage, however, is that we can then potentially avoid having to enable `if` statements to work with non-boolean 
values. However, I'm not convinced this would be a good idea as having conditions working differently depending on their context could be confusing
for the user.


## Outstanding Questions

- One thing that the `swimlanes` can do that this RFC hasn't catered for is that `swimlanes` can send the event to different sinks depending on a condition. 
With Remap there is only one possible component, another transform or a sink, that can accept the output of the mapping. Is this something that should 
be catered for by the Remap language and this RFC?


## Plan Of Attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ] Add `else if` to the remap language parser and add multiple branches to the `IfStatement` struct to handle multiple conditions.
- [ ] Change the `IfStatement` functionality to treat non-boolean values as pass and fail for the conditional.
- [ ] Enhance the remap parser to allow assignment within `if` statements. The underlying code already allows for assignments to 
      pass on the assigned value to the outer expression, so no change should be necessary there.
- [ ] Write the `matches` function that will run a regular expression and return any matched groups found. This is potentially out of scope
      for this rfc, but would be useful nevertheless.

