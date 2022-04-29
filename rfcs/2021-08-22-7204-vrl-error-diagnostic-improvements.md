# RFC 7204 - 2021-08-22 - VRL Error Diagnostic Improvements

We implement a list of improvements to VRL's error diagnostics, to make it
easier to understand what went wrong when a program fails to compile, and how to
solve the problem.

<!-- vim-markdown-toc GFM -->

* [Context](#context)
* [Cross cutting concerns](#cross-cutting-concerns)
* [Scope](#scope)
  * [In scope](#in-scope)
  * [Out of scope](#out-of-scope)
* [Proposed Solutions](#proposed-solutions)
  * [Prevent Incorrect Error Diagnostics](#prevent-incorrect-error-diagnostics)
    * [Pain](#pain)
    * [User Experience](#user-experience)
    * [Implementation](#implementation)
  * [Recover From Non-Fatal Expression Errors](#recover-from-non-fatal-expression-errors)
    * [Pain](#pain-1)
    * [User Experience](#user-experience-1)
    * [Implementation](#implementation-1)
  * [Remove Generic "Expression Can Result in Runtime Error" Diagnostic](#remove-generic-expression-can-result-in-runtime-error-diagnostic)
    * [Pain](#pain-2)
    * [User Experience](#user-experience-2)
    * [Implementation](#implementation-2)
  * [Correctly Diagnose Invalid Function Argument Types](#correctly-diagnose-invalid-function-argument-types)
    * [Pain](#pain-3)
    * [User Experience](#user-experience-3)
    * [Implementation](#implementation-3)
  * [Update VRL Error Codes](#update-vrl-error-codes)
  * [Update VRL Website Documentation](#update-vrl-website-documentation)
* [Rationale](#rationale)
* [Drawbacks](#drawbacks)
* [Outstanding Questions](#outstanding-questions)
* [Plan Of Attack](#plan-of-attack)
* [Future Improvements](#future-improvements)

<!-- vim-markdown-toc -->

## Context

* [RFC 4862 - 2020-11-02 - Remap Language Compile-Time Type Checking v1][#4862]

[#4862]: https://github.com/vectordotdev/vector/blob/ab9ff57ddccaa561b84eb3d0139bd764ad655fa6/rfcs/2020-11-02-remap-language-compile-time-type-checking-v1.md

## Cross cutting concerns

* [Log schemas][#3910]
* [Improve VRL type checking][#8380]
* [VRL compiler improvements][#8221]

[#3910]: https://github.com/vectordotdev/vector/issues/3910
[#8380]: https://github.com/vectordotdev/vector/issues/8380
[#8221]: https://github.com/vectordotdev/vector/issues/8221

## Scope

### In scope

* Improve VRL error diagnostics
* Simplify VRL error codes
* Update VRL website documentation

### Out of scope

* Changes to error handling in VRL itself
* Non-diagnostics changes to the VRL compiler

## Proposed Solutions

Given that this RFC proposes to implement multiple solutions to improve error
diagnostics, the following chapters combine both the "pain", "user experience"
and "implementation" sections _per proposed solution_.

### Prevent Incorrect Error Diagnostics

#### Pain

When a VRL program is compiled, it accumulates compilation errors before showing
a list of diagnostic messages to the operator. We do this, so that operators can
resolve multiple errors at once without having to recompile after each
individual error.

The existing implementation to support this is very simplistic, and can result
in the compiler injecting _new_ errors that aren't caused by the operator.
Specifically, if the compiler comes across an invalid expression, it substitutes
the expression with a no-op variant, such that compilation can continue. This
no-op expression can cause follow-up expressions to become invalid as well.

Take this VRL program:

```coffee
foo = to_strng(15)
upcase(foo)
```

There's a typo (`to_strng` instead of `to_string`), which causes the following
diagnostic message:

```text
error[E105]: call to undefined function
  ┌─ :1:7
  │
1 │ foo = to_strng(15)
  │       ^^^^^^^^
  │       │
  │       undefined function
  │       did you mean "to_string"?
```

However, because the compiler wants to continue compiling the rest of the
program, it replaced the faulty `to_strng(15)` expression with a `null`
expression, before continuing with the compilation. This results in the
follow-up expression `upcase(foo)` to fail, because `upcase` only accepts
strings as its type, which would be the case if the operator had correctly used
`to_string`, but not with the substituted expression of `null` the compiler
injected.

This results in the following invalid diagnostic message:

```coffee
error[E110]: invalid argument type
  ┌─ :1:8
  │
1 │ upcase(foo)
  │        ^^^
  │        │
  │        this expression resolves to the exact type "null"
  │        but the parameter "value" expects the exact type "string"
  │
  = try: coercing to an appropriate type and specifying a default value as a fallback in case coercion fails
  =
  =     foo = to_string(foo) ?? "default"
  =     upcase(foo)
  =
  = try: ensuring an appropriate type at runtime
  =
  =     foo = string!(foo)
  =     upcase(foo)
```

In this case, it isn't immediately obvious why `foo` is expected to resolve to
type `null`, since the operator never intended `null` to be assigned.

If the operator were to follow the advice listed in this diagnostic message,
they would end up with a compilation error again, because the compiler gave them
incorrect advice:

```coffee
foo = to_string(15)
foo = string!(foo)
upcase(foo)
```

```text
error[E620]: can't abort infallible function
  ┌─ :1:7
  │
1 │ foo = string!(foo)
  │       ^^^^^^- remove this abort-instruction
  │       │
  │       this function can't fail

error[E110]: invalid argument type
  ┌─ :1:8
  │
1 │ upcase(foo)
  │        ^^^
  │        │
  │        this expression resolves to the exact type "null"
  │        but the parameter "value" expects the exact type "string"
  │
  = try: ensuring an appropriate type at runtime
  =
  =     foo = string!(foo)
  =     upcase(foo)
  =
  = try: coercing to an appropriate type and specifying a default value as a fallback in case coercion fails
  =
  =     foo = to_string(foo) ?? "default"
  =     upcase(foo)
```

Because `to_string` is now valid, it returns a `string` type, which means
`string!(foo)` is no longer needed, and in fact is disallowed, because that
function call is infallible, and so using `!` is unneeded.

As you can see, this results in a never-ending list of diagnostic messages,
until the operator discovers why this is happening, and instead ignores the
second compiler diagnostic message, only fixing the function-call typo, and
leaving the rest of the program as-is.

This is a sub-par experience.

#### User Experience

The VRL compiler does not modify the original program in a way that previously
non-existing error messages outside the control of the operator are introduced.

#### Implementation

Currently, individual program expressions are compiled using `compile_*`
functions, e.g.:

```rust
fn compile_function_call(&mut self, node: ast::FunctionCall) -> FunctionCall {
    // ...
}
```

Within those functions, errors are tracked by pushing new errors to
a `self.errors` array.

To support tracking the first-encountered error, but ignore future errors within
the same expression chain, we update these calls to return `Option<T>` instead:

```rust
fn compile_function_call(&mut self, node: ast::FunctionCall) -> Option<FunctionCall> {
    if /* invalid */ {
        self.errors.push(/* ... */);
        return None
    }

    // ...
}
```

If such a function returns `None`, it means the compiler is to ignore the given
expression in its final output.

This modification makes further required changes in the compiler minimal, since
we can use Rust's "error propagation" operator (`?`) to exit early if a nested
expression is to be ignored:

```rust
fn compile_function_call(&mut self, node: Node<ast::FunctionCall>) -> Option<FunctionCall> {
    // ...

    // stop compiling the function if one of its arguments is invalid.
    let arguments = arguments
        .into_iter()
        .map(|node| self.compile_function_argument(node))
        .collect::<Option<_>>()?;

    // ...
}
```

Alternatively, instead of `Option<T>` we could use `Result<T, E>`. But there
doesn't seem to be any value in doing so right now, and changing from the former
to the latter should be easy enough in the future if needed.

For this solution to work with the above example, we need to track the status of
expressions in the variable type definition as well. That is, any variable
assignment expression for which the right-hand expression returns `None`, needs
to store this fact, such that future expressions referencing this variable, are
also ignored during compilation.

Currently, we track variable state like so:

```rust
mod state {
  pub struct Compiler {
      /// Stored internal variable type definitions.
      variables: HashMap<Ident, assignment::Details>,

      // ...
  }
}
```

We'll change this to store `Option<assignment::Details>`. When `None` is stored,
this indicates that there _is_ a variable for the given name that gets assigned
a value, but due to an error the value should be ignored.

This allows us to differentiate between a non-existing variable (resulting in
a "undefined variable" diagnostic error), or an existing variable that we're
ignoring because of previous compilation errors.

### Recover From Non-Fatal Expression Errors

#### Pain

Given the following VRL program:

```coffee
bar = to_string(invalid: 12)
upcase(bar)
```

This program fails, because `to_string` takes one argument `value`, not
`invalid`:

```text
error[E108]: unknown function argument keyword
  ┌─ :1:17
  │
1 │ bar = to_string(invalid: 12)
  │       --------- ^^^^^^^ unknown keyword
  │       │
  │       this function accepts the following keywords: "value"

error[E110]: invalid argument type
  ┌─ :1:8
  │
1 │ upcase(bar)
  │        ^^^
  │        │
  │        this expression resolves to the exact type "null"
  │        but the parameter "value" expects the exact type "string"
```

Here too, the invalid function call `to_string` is substituted with a default
`null` value, resulting in the same subsequent error message.

In this case however, because the function definition is known, we _can_ know
the return type of the call (in this case, a `string`). Because of this - after
we track the invalid parameter name error - we can pretend as if the function
call succeeded, regardless of whether the function argument was set incorrectly.

#### User Experience

The VRL compiler can continue to compile a program without unexpected errors,
even if a known expression is used incorrectly. The compiler continues as if the
expression resolved correctly, using the return type of the expression itself.

#### Implementation

When a compiled expression returns an error, it checks to know if its type
— assuming no error had occurred — can be known. If it can, it compiles a "dummy
expression" with the same type, allowing compilation of the program to continue,
and other errors to be tracked.

```rust
fn compile_function_call(&mut self, node: Node<ast::FunctionCall>) -> Option<FunctionCall> {
    let func = /* ... */;

    // This can return "none" if one of the passed in argument expressions fail.
    let arguments = arguments
        .into_iter()
        .map(|node| self.compile_function_argument(node))
        .collect::<Option<_>>();

    let arguments = match arguments {
        Some(arguments) => arguments,
        // If an argument failed to compile, we can still compile the function,
        // by using a no-op variant, using the same type-def and function name
        None => return Some(FunctionCall::noop(func.ident, func.type_def)),
    };

    // ...
}
```

In the example above, the arguments to a function call can be incorrect, but the
function can still relay the type(s) it returns. This then allows us to swap out
the actual function call with a no-op call, which does nothing, except that it
mimics the type definition and name of the actual function.

Note that `FunctionCall::noop` already exists, but it currently implemented as
such:

```rust
impl FunctionCall {
    pub fn noop() -> Self {
        let expr = Box::new(Noop) as _;

        Self {
            ident: "noop",
            expr,
            // ...
        }
    }
}
```

We'd update this to something like:

```rust
// Compile a no-op function that is defined to return the given value kind, and
// identifies itself with the given function name.
pub fn noop(ident: &'static str, kind: value::Kind) -> Self {
    let expr = Box::new(Noop(kind)) as _;

    Self {
        ident,
        expr,
        // ...
    }
}
```

With this change, the error for the invalid function is captured, but the
function itself is turned into a "valid" one, using the same name and
return-type, but without the actual underlying implementation (e.g. its call is
a no-op).

Given that the error itself is captured, the compiler will still fail after it
finished compilation, but it continues compiling follow-up expressions to allow
more errors to accumulate.

### Remove Generic "Expression Can Result in Runtime Error" Diagnostic

#### Pain

The "expression can result in runtime error" diagnostic is an error message that
occurs when a root level expression is considered fallible:

```coffee
to_string(.foo)
```

```text
error[E100]: unhandled error
  ┌─ :1:1
  │
1 │ to_string(.foo)
  │ ^^^^^^^^^^^^^^^
  │ │
  │ expression can result in runtime error
  │ handle the error case to ensure runtime success
```

The above example is fairly straightforward, but when the number of nested
expressions gets deeper, it becomes more difficult to understand what is
happening:

```coffee
if (parsed  = parse_grok(.message, "%{GREEDYDATA:parsed}"); parsed != null) {
  merge(., parsed)
}
```

The same error is raised when you try to divide by zero:

```coffee
1 / 0
```

Or when you group multiple expression types:

```coffee
5 + to_int(.foo)
```

The issue we need to solve, is that we can't know by looking at a single
expression whether or not the root level expression will be fallible.

Take this example:

```coffee
to_int(.foo)
```

On its own, `to_int(.foo)` is fallible, because we don't know the eventual value
of `.foo` at runtime, and because `to_int` can be fallible for certain input
types, we don't know if `to_int` can fail at compile-time.

However, when compounding the function call expression with an error-coalescing
expression, we can make the root level expression infallible:

```coffee
to_int(.foo) ?? 0
```

Now, if `to_int` were to fail, it would default to `0`, and thus the root-level
expression becomes infallible.

Because of this, we have a catch-all "expression can result in runtime error"
error, which is used when we can't determine if an individual expression itself
needs to be infallible for the program to compile, but we _can_ determine if
each individual root expression (the combination of all expressions on a given
source code line) is infallible or not.

The problem is, this error is ambiguous, and gives no clear direction on how to
solve the problem.

Take this example:

```coffee
"foo" + .bar + baz[1]
```

```text
error[E100]: unhandled error
  ┌─ :1:1
  │
1 │ "foo" + .bar + baz[1]
  │ ^^^^^^^^^^^^^^^^^^^^^
  │ │
  │ expression can result in runtime error
  │ handle the error case to ensure runtime success
```

It is not immediately clear which part of this program is fallible. It's either
assigning `.bar.` to `"foo"`, or `baz[1]` to `"foo" + .bar`.

This is still a fairly simple example, but you can make it as complex as you
want, making it more and more difficult to understand what needs fixing.

#### User Experience

Whenever fallibility of an expression prevents a program from compiling, the
compiler must always point to the exact expression that causes the problem,
instead of highlighting the entire chain of expressions.

#### Implementation

Given the following nested expressions:

```text
A ( B ( C ( D ) ) )
```

Where:

* `A` is the root expression
* `C` is fallible
* `D` is infallible

The compiler must return a diagnostic similar to this:

```text
error[E100]: unhandled error
  ┌─ :1:1
  │
1 │ A ( B ( C ( D ) ) )
  │         ^^^^^^^
  │         │
  │         C expression is fallible
  │         handle the error case to ensure runtime success
```

Here's what the compiler will do:

* Compile the nested expressions, from `D` to `A`
* `D` is infallible, and thus valid
* `C` is fallible
  * But it might become infallible by a parent expression
  * Set `C` as the source of fallibility for this expression chain
* `B` is fallible
* `A` is fallible
  * This is the root expression, and thus the fallibility of `C` remains
    unhandled
* Show the error diagnostic specific to the unhandled fallibility of `C`

The compiler has a `compile_root_exprs` function:

```rust
fn compile_root_exprs(
    &mut self,
    nodes: impl IntoIterator<Item = Node<ast::RootExpr>>,
) -> Vec<Expr> {
    // ...
}
```

This function is called first to compile a VRL program. For each root
expression, it does something like the following:

```rust
for expr in root_expressions {
    let expr = self.compile_expr(expr);

    if expr.is_fallible() {
        self.errors.push(ExpressionError);
    }
}
```

This will need to be changed. Instead, we'll add an extra piece of state to the
compiler, with the error to show _if_ a root expression remains infallible,
something like:

```rust
for expr in root_expressions {
    self.fallible_expression_error = None;

    let expr = self.compile_expr(expr);

    if let Some(error) = self.fallible_expression_error {
        self.errors.push(error);
    }
}
```

The `fallible_expression_error` state will be set by the first expression in the
chain (going from last to first) that is infallible, and will be set back to
`None` by any subsequent expression that is infallible (indicating that the
fallibility of the child expression is nullified).

If any expression further up the chain is marked as fallible again, the value
becomes `Some(T)` again, until we've reached the root expression, which is
handled by the above code snippet.

This allows us to be specific about which part of a VRL program is fallible, and
needs to be rectified.

### Correctly Diagnose Invalid Function Argument Types

#### Pain

Because of [_reasons_][#6507] we allow function arguments to be given a type
that the function itself doesn't accept. When this happens, a function becomes
"fallible" and has to be handled.

This makes it easier to do things like `sha3(.foo) ?? null`.

In the above example, the final VRL program is infallible, because we handle the
fallibility of `sha3` with `?? null`. The reason `sha3` is fallible, is because
we pass in a type that it potentially doesn't support - it only accepts strings,
but at compile-time we don't know the type of `.foo` (it might be a string, or
it might not be).

In an earlier version of VRL, you had to guarantee that `.foo` was a string at
compile-time, before being allowed to pass it into `sha3` as an argument. We
backed out of that, because it meant you had to write a whole bunch of ugly
`sha3(string!(.foo))` code (or, preferably, handle the error case of `.foo` not
being a string, requiring even more verbose programs to be written).

Instead, we allow function arguments to be of any type, but if the type doesn't
adhere to the type the function expects, you have to make the function call
itself infallible (using any of our supported error-handling logic, which is why
`?? null` works).

The big downside to this, is that we document our functions to be either
fallible, or infallible. A function is infallible, if it can never fail at
runtime.

In practice though, because of the above mentioned design decision, _any_
function that takes one or more arguments, and those arguments have a limit on
which types they accept, can become a fallible function call.

This results in people being confused, as they call `upcase(.foo)`, expect it to
be infallible (because up casing a string never fails), but instead they get an
error indicating that their program is fallible, and they need to use (for
example) `upcase!(.foo)` (note the `!`).

We need to come up with a solution that prevents the issues we had before with ugly
type checking code, but the diagnostic still informs operators on why their
function call fails.

[#6507]: https://github.com/vectordotdev/vector/issues/6507

#### User Experience

When an infallible function becomes fallible because of an invalid argument
type, the compiler will explain this situation in the diagnostic, to avoid any
user confusion.

Additionally, the website documentation will be updated to ensure users are
aware of this specific behavior of the VRL compiler.

#### Implementation

We already [track][] function arguments that might be of the incorrect type, by
having a `maybe_fallible_arguments` private field on the `FunctionCall`
expression. This is currently used by the compiler to mark the function call as
fallible if any of its arguments can be incorrect.

We're going to need to extend this feature to allow us to track exactly which
argument is potentially fallible, in order to return a more exact error.

Additionally, this change depends on the change discussed in [Remove Generic
"Expression Can Result in Runtime Error"
Diagnostic](#remove-generic-expression-can-result-in-runtime-error-diagnostic),
as it requires us to show a specific "argument might be of invalid type" error
_only_ if the user hasn't already handled the potential error by making the
function call infallible.

First, we change this:

```rust
struct FunctionCall {
    maybe_fallible_arguments: bool,

    // ...
}
```

To become:

```rust
struct FunctionCall {
    arguments_with_unknown_type_validity: Vec<Node<FunctionArgument>>,

    // ...
}
```

This allows us to track more details about each individual argument that might
have an invalid type (including the parameter name, argument types, and their
span within the source code).

We'll also update any code that did a boolean check for
`maybe_fallible_arguments` to
`!arguments_with_unknown_type_validity.is_empty()`.

Next, in the compiler's `compile_function_call`, if the function call has any
potentially invalid argument types, but is otherwise valid, we set the
compiler's `fallible_expression_error` (as introduced in this RFC) to a new
`MaybeInvalidArgumentType` error, pointing to the exact location in the source
for the given argument, and explaining which types the function parameter
expects, and which types the argument can expand to at runtime.

Something similar to this:

```text
error[E100]: argument type might be invalid at runtime
  ┌─ :1:8
  │
1 │ upcase(.foo)
  │        ^^^^
  │        │
  │        the parameter "value" expects the exact type "string"
  │        but the expression ".foo" can resolve to either a "string" or "null" at runtime
  │
  = try: ensuring an appropriate type at runtime
  =
  =     foo = string!(.foo)
  =     upcase(.foo)
  =
  = try: coercing to an appropriate type and specifying a default value as a fallback in case coercion fails
  =
  =     .foo = to_string(.foo) ?? "default"
  =     upcase(.foo)
  =
  = see documentation about error handling at https://errors.vrl.dev/#handling
  = learn more about error code 100 at https://errors.vrl.dev/100
  = see language documentation at https://vrl.dev
```

[track]: https://github.com/vectordotdev/vector/blob/ab9ff57ddccaa561b84eb3d0139bd764ad655fa6/lib/vrl/compiler/src/expression/function_call.rs#L241-L299

### Update VRL Error Codes

We introduced "error codes" right before we launched VRL itself. Since then, we
haven't really paid any attention to them. We should go through each of them,
make sure all errors have an error code, that the documentation surrounding the
error codes is still accurate, and remove any error codes that aren't useful.

### Update VRL Website Documentation

* Make sure all diagnostic errors are covered on the website
* Update function documentation, to make it clear that an infallible function
  can become fallible if its arguments are potentially invalid.

## Rationale

Writing VRL programs incurs some upfront cost by design. We want programs to be
infallible at runtime, which means forcing the handling of potential errors by
operators when they write a VRL program. This means operators likely have to
deal with multiple compilation errors before the compiler is satisfied with
their provided program.

The more accurate and helpful these errors are, the less friction there is for
operators, and the less likely they require help from the Vector team, or give
up on Vector entirely due to frustrations with VRL.

By improving the ergonomics of error handling, we reduce the upfront cost of
writing a Vector config that includes a VRL program, in turn facilitating
a faster "time to market", and improving happiness of the Vector user base.

## Drawbacks

Other than the cost required to do the work, the other drawback is an increase
in compiler complexity, as it has to track more state as errors are caught and
converted to use-able diagnostic messages.

## Outstanding Questions

None.

## Plan Of Attack

* [ ] Prevent Incorrect Error Diagnostics
* [ ] Recover From Non-Fatal Expression Errors
* [ ] Remove Generic "Expression Can Result in Runtime Error" Diagnostic
* [ ] Correctly Diagnose Invalid Function Argument Types
* [ ] Update VRL Error Codes
* [ ] Update VRL Website Documentation

## Future Improvements

* `vrl check` subcommand
* `vrl fix` subcommand to auto-fix errors
* improve error diagnostic visuals
* undefined variable checking
* non-error diagnostics (for example when using functions that can be slow)
* more/better "try" solutions in error diagnostics
