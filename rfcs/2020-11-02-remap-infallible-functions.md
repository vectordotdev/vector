# RFC … - 2020-11-02 - Remap Language Fallible and Infallible Functions

By introducing a distinction between fallible and infallible functions, we can
provide more runtime guarantees to users of the Remap language.

## Scope

- The RFC describes how users can call both fallible and infallible variants of
  functions.
- It does so through a more broader context of fallible and infallible
  _expressions_
- It _does not_ decide on which expressions (other than functions) should
  support the concept of infallibility.

## Motivation

As we expand the use-case for the Remap language within Vector, we want to be
able to guide users as much as possible as they have their first experience with
the language in Vector.

One way to do so is to have programs run _as predictable as possible_. This
means moving errors as much as possible to boot-time, while reducing the list of
potential runtime exceptions.

This RFC proposes a way to allow users to determine how they want to handle
runtime errors for each individual function, so that they can decide if an error
is truly an exception and should abort program execution, or if the error can
safely be ignored.

## Internal Proposal

The following Remap program is easy to understand:

```javascript
.message = parse_json(.message)
.timestamp = now()
```

It:

1. Reads the value of the `message` field of the object.
2. Parses the value to a JSON value (a literal, array, or map).
3. Assigns the new value to the `message` field, removing the old value.
4. Assigns the current timestamp to the `timestamp` field.

### Fallible Functions

While the program itself is easy to understand, running it can result in
unexpected situations.

If whatever value the `message` field contains cannot be parsed as valid JSON,
the `parse_json` function returns an error.

This results in an outcome where the original event is left untouched, the
`message` field still contains the original value, and no `timestamp` field
exists (assuming none existed to start with).

A use-case such as the above is quite common. Fields often need to be parsed as
JSON values, but at the same time those fields are highly dynamic and might not
always be considered valid JSON, for example if a bug in an application starts
generating invalid stringified JSON output.

Users need a way to handle such situations, without having to resort to complex
solutions.

### Infallible Function Variants

The solution proposed by this RFC is to introduce the concept of _infallible
function variants_.

An infallible function variant is an exact copy of an existing fallible
function, but it is guaranteed to never fails.

The infallible function must be as easy (or easier) to call than the fallible
function, without any significant syntactic complexity.

### To Bang Or Not To Bang

The proposal is to introduce the concept of the bang (`!`) function identifier
additive to determine as a user if a function should be allowed to fail or not.

Function identifiers with a bang are considered to be fallible, while functions
without are not:

```javascript
.message = parse_json(.message) // cannot fail
.counter = 1 + to_int!(.counter) // allowed to fail
.timestamp = now()
```

In the above example, `parse_json` is written without a bang, and will thus
never fail, whereas `to_int!` includes the bang, and will fail the program (and
leave the event unchanged) if `.counter` cannot be coerced into an integer.

### Infallible Function Result

So far so good, but telling a program you don't want JSON parsing the fail does
not magically make any input valid JSON.

Given the original example, any badly formatted input values are still
considered invalid JSON and the parser is thus unable to return a representative
JSON value.

So what should the function return instead?

Infallible functions are allowed to return `Option<Value>`. In other words, they
can return "nothing", if there isn't anything useful to return.

### How To Handle "Nothing"

Let's take a look at the original example again:

```javascript
.message = parse_json(.message)
.timestamp = now()
```

If `message` contains any data that `parse_json` can't handle, it will return
`None`.

How that `None` gets processed is up to the expression that processes the value.

For example, in the case of the `Assignment` expression (which is what's called
when doing `.message = …`), `None` values skip assigning the value to the target
path, returning `None` itself.

Given this, the outcome of the above example would change from "exception,
original event untouched", to "JSON parsing failed, `message` field untouched,
`timestamp` field set to current timestamp".

### Explicitly Handle Error Case

While the above provides more flexibility to the end-user on when to terminate
the program and when not to, it still does not allow the user to detect if JSON
parsing failed.

To do this, we modify the `IfStatement` and `Logical` expressions to support
falling though to the "else" expression if the conditional expression returns
`None`.

For example:

```javascript
if ($message = parse_json(.message)) {
    // parsing succeeded
} else {
    // parsing failed
}
```

### False vs. "Nothing"

One issue with this proposal is that `false` is a valid JSON value, and so the
above check does not distinguish between false values or failed JSON parsing.

One solution to this is to _not_ update `IfStatement`, but instead introduce a
new `is_undefined` function, which returns `true` for `None` values, or else
returns `false` (or inversely, we add a `is_defined` function):

```javascript
if ($message = parse_json(.message); is_undefined($message)) {
    // parsing failed
} else {
    // parsing succeeded
}
```

It should be noted that this example is flawed, because it expects `$message` to
be undefined if the parsing failed. Earlier in the examples we documented that
the assignment expression would not assign to the target if the resolved value
is `None`. Meaning, if `$message` was already defined earlier in the program, it
would remain defined to that original value, and the `is_undefined` check would
return `false`, unexpectedly.

One could avoid this by doing parsing twice:

```javascript
if parse_json(.message) {
    // parsing succeeded, do it again!
    .message = parse_json(.message)
}
```

This comes at a performance cost, as the message has to be parsed twice.

An alternative is to consider assigning "nothing" to a target as erasing that
target, such that `.foo = parse_json(.foo)` would resolve in `.foo` being erased
if `parse_json` failed, which in turns makes `is_undefined` work as expected.

### Implementation Details

The above examples are light on implementation details, but on a technical
level, this logic will be implemented as follows.

A new `Infallible` expression type is implemented, which works as such:

```rust
struct Infallible(Box<dyn Expression>);

impl Expression for Infallible {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        &self.0.execute(self, state, object).and_then(|r| match r {
            Ok(maybe) => Ok(maybe),
            Err(_) => Ok(None), // potentially emit debug event here
        })
    }
}
```

The parser detects when a function call identifier does not end with a bang
(`!`), and wraps the function expression in the `Infallible` expression.

Everything else keeps working as before.

### Fallible Call to Infallible Functions

Some functions are infallible by definition, there is no way in which they can
fail. For example, `now()` cannot fail, and neither can `to_string(.foo)`, no
matter the value of `.foo`.

We _could_ expand the `Function` trait as such:

```rust
pub trait Function: Sync {
    /// Whether the function can fail during execution.
    fn is_fallible(&self) -> bool {
        true
    }

    // existing trait signature
    fn identifier(&self) -> &'static str;
    fn compile(&self, arguments: ArgumentList) -> Result<Box<dyn Expression>>;
    fn parameters(&self) -> &'static [Parameter] {
        &[]
    }
}
```

Which would allow us to return a compile-time error if a function is called as
fallible, when it can never fail (e.g. `.timestamp = now!()` would not compile).

On the one hand, this seems like a nice-to-have to educate users what they can
expect from a function, on the other hand it could be considered overly
pedantic, since the user explicitly said "I'm okay if this fails" by adding the
bang at the end, it doesn't say anything about if the function _will_ fail.

Because of this, adding this is considered to be **out of scope** for the
initial implementation, but it is purely _additive_, and can be added later with
relative ease.

### Infallible Query Paths?

Given that functions are expressions, but any other snippet of Remap code is
also an expression, means that we _could_ extend this capability to path queries
as well.

For example, one could write:

```javascript
.foo.bar
.foo.bar!.baz
```

On the first line, if `.foo` does not exist, `None` is returned for the
`Path` expression.

For the second line, if `foo` does not exist, `None` is returned, but if `bar`
does not exist, the expression fails with an exception.

Infallible query paths are considered **out of scope** for this RFC, but map
well onto the existing proposal with only minor modifications needed.

### Infallible Or Fallible By Default

The current RFC proposes to make infallible functions the default (e.g.
functions without a bang will never fail). It requires an explicit decision by
the user to add the `!` character and allow the function to fail.

While the idea is that this is the most simple solution to the problem, there is
a case to be made for the fact that this can unintentionally silently ignore
errors that should be caught, if one forgets to add the `!` character, or simply
isn't aware of the existence of fallible and infallible function variants.

It is therefor possible to decide that we want the inverse to happen, without a
bang the function fails at runtime, with a bang, you accept the suppression of
errors (and potentially manually handle those errors).

### Manual Abort

If a user wants to manually abort the program for a given failure, they
currently can't.

Take this example:

```javascript
if ($message = parse_json(.message); is_undefined($message)) {
    // parsing failed

    if .foo > 5 {
        .message = "failed, but foo was greater than 5, so all's good!"
    } else {
        // abort, abort!
    }
} else {
    // parsing succeeded
}
```

We could add an `abort` or `fail` function which takes a string, and returns an
error containing the given string.

This would allow one to write:

```rust
if /* condition */ {
    // success
} else {
    abort("failed, because foo was less than 5, we really can't let this slide!")
}
```

This change is currently **out of scope** for this proposal.

## Doc-level Proposal

- If a user wants event processing to fail if a function fails, they add a bang
  (`!`) to the function identifier (before the parenthesis and its arguments).
- They can manually detect if a function fails through if statements and
  `is_undefined`.

## Rationale

Remap relies heavily on functions to map object data. A lot of these functions
can fail, and if one fails, all subsequent operations on the object are aborted.

This can result in unexpected outcomes, which we want to avoid as much as
possible.

The proposed change is fairly limited in scope, it adds a bit of (additive)
syntax change to the language, only a minor refactor to the parser, and a simple
implementation of the new `Infallible` expression.

All in all, the changes required are minimal and non-invasive, while the upside
is more control for the end-user on when to fail program execution and when not
to.

## Prior Art (TODO)

## Drawbacks

- added complexity to the language
- "safe by default" might obscure errors
- TODO

## Alternatives (TODO)

## Outstanding Questions

- Should we use "safe by default" or "fallible by default"?
- Should assigning an undefined value result in no action, or removing any
  potential existing value of the target path/variable?
- How should if statements and logical operators handle undefined values?
- Should we provide compile-time error for fallibly calling always-infallible
  functions (e.g. `now!()`)?
- Should we add support for (in)fallible path queries in this RFC?
- Should we add support for manual abort function in this RFC?

## Plan Of Attack (TODO)
