# RFC 4862 - 2020-11-02 - Remap Language Compile-Time Type Checking v1

We extend the Remap language to be able to determine at compile-time which value
type(s) a program returns on completion, so that we have less runtime failure
conditions when using Remap within Vector.

## Scope

This RFC proposes extending the `Expression` trait. It also explains how we use
these changes when using Remap in the context of a Vector `Condition`. It will
hint at future uses, but does not consider them in-depth.

The proposal does **not** consider fallible vs infallible expressions, those are
left for a future RFC.

It does also **not** consider using compile-time type checking for function
arguments, although the plumbing introduced through this RFC will make such a
thing possible in a follow-up RFC.

## Motivation

As we expand the use-case for the Remap language within Vector, we want to be
able to guide users as much as possible as they have their first experience with
the language in Vector.

Since the language is used in different contexts, that expect different outcomes
of a program, the chances of the program failing at runtime grows, which can
result in undetected failures if the log output of Vector isn't observed.

We should strive for as much "boot-time" errors as possible, so that problems
are detected early-on, before any data processing starts.

Specifically, we support using the Remap language within Vector in any context
that takes a so-called "condition". These conditions allow components to perform
a check against an event, and return a boolean outcome. For example, in the
`swimlanes` transform, the check determines if an event should be included in a
specific swimlane.

Currently, any non-boolean return value of a remap program results in a failure
of the condition, which can be confusing to users.

## Terminology

- `source` — The Remap source code written by users, to compile and execute.
- `program` — A list of expressions, compiled from source and a list of
  functions, ready to run against an object.
- `expression` — A piece of source code that resolves to a value.
- `value` — a concrete value type, such as string, integer, boolean, etc.
- `object` — A generic term for whatever outside data container the program
  manipulates (such as an "event").

## Internal Proposal

Remap is an expression-based language. Everything, including statements, is an
expression. Meaning, everything can return a value.

### expression examples

For example, the program `.foo = "bar"` consists of two expressions:

1. The literal expression `"bar"`, which resolves to a `Value::String`.
2. The assignment expression `.foo = …`, which resolves to the value of the
   expression on the right side of the assignment operator.

This example program, when executed, always resolves to a `String` value.

Another example program is `floor(.bar)`. This program again consists of two
expressions:

1. The query path expression `.bar`, which resolves to any value the `bar`
   object field contains.
2. The function expression `floor`, which resolves to either an `Integer`, or
   `Float`.

Since the `floor` function is the final expression that runs in the program, the
outcome of a _successful execution_ of the program is either an `Integer` or
`Float`.

### expression trait

Expressions are implemented using the `Expression` trait:

```rust
pub trait Expression: Send + Sync {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>>;
}
```

Expressions can be fallible (not covered in this RFC), can return `None` (not
covered in-depth in this RFC), or return a resolved `Value`.

### defining the expected resolved values

To allow the execution context of a program to determine the outcome of running
the program, we extend the `Expression` trait as follows:

```rust
pub trait Expression: Send + Sync {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>>;

    /// Describes which [`ValueKind`]s the expression can resolve to.
    ///
    /// bike-shedding of function and enum names welcome.
    fn resolves_to(&self) -> ResolveKind;
}
```

We define `ResolveKind` as follows:

```rust
pub enum ResolveKind {
    /// The expression can resolve to any value at runtime.
    Any,

    /// The expressions resolves to one of the defined [`ValueKind`]s.
    OneOf(Vec<ValueKind>),

    /// If the expression succeeds, it might resolve to a value, but doesn't
    /// have to.
    Maybe(Box<ResolveKind>),
}
```

The `ResolveKind` enum allows us to determine if an expression resolves to a
predefined list of values. It also defines if the expression can resolve to
"nothing" (e.g. `None`).

`ValueKind` is defined as follows:

```rust
pub enum ValueKind {
    String,
    Integer,
    Float,
    Boolean,
    Timestamp,
    Map,
    Array,
}
```

As can be seen from the `ValueKind`, it is not possible to define the value
kinds contained within a map or array, only the top-level value kind of an
expression can be defined.

### transient expressions

Some expressions don't have their own return value, but instead resolve to the
return value of an expression they themselves execute.

One such an example is the `IfStatement` expression, which resolves to either
the `true_expression` or the `false_expression`.

Because expressions are initialized at compile-time, we are able to have such
"transient expressions" return the values they resolve to based on whatever
expressions they themselves hold:

```rust
impl Expression for IfStatement {
    fn resolves_to(&self) -> ResolveKind {
        let true_resolves = self.true_expression.resolves_to();
        let false_resolves = self.false_expression.resolves_to();

        // return the combined set of true and false resolves.
    }

    // …
}
```

The same applies to other expressions, such as `Assignment`, `Block`, and more.

### defining expected outcomes

To allow a program to define the expected outcome, we expand the `Program`
struct.

This struct currently looks like this:

```rust
pub struct Program {
    pub(crate) expressions: Vec<Expr>,
}
```

To initialize a program, you call the `new` function:

```rust
pub fn new(source: &str, function_definitions: &[Box<dyn Function>]) -> Result<Self>;
```

The program then parses the source to a list of expressions, the final
expression resolving to the result of the program.

We'll extend the program to allow defining which value kinds the final
expression is allowed to resolve to:

_(note that a Remap program always runs to completion, unless one of the earlier
expressions returns an error, in which case there is no resolved value)_

```rust

pub struct Program {
    pub(crate) resolves_to: ResolveKind.
    pub(crate) expressions: Vec<Expr>,
}
```

This allows us to check the `ResolveKind` of the final expression against the
configured one for the program. If the former falls within the bounds of the
program, the program is considered valid, and no boot-time error occurs, or else
we inform the user that they need to update their script to return a more
precise value kind.

### Variable and Path Value Kinds

One more example we need to consider is variable or path assignments, and how
those influence compile-time type checking.

Given this example:

```rust
$foo = true
$foo
```

This produces three expressions:

1. The `Literal` expression `true`, which is of type `Boolean`.
2. The `Assignment` expression `$foo = …` which returns a type `Boolean`.
3. The `Variable` expression `$foo`, which returns type `Boolean`.

Given the above expressions, it follows that this program will return a
`Boolean` value.

However, this is not the case.

The current implementation of the `Assignment` expression stores the target
identifier (`foo`), and the expression to store (`true`) at compile-time. At
runtime, it resolves the expression, and stores the resulting value in a global
runtime store, either as a variable or path with the identifier `foo`.

The `Variable` expression _also_ stores the target identifier (`foo`) at
compile-time. At runtime it queries the global runtime store for a variable
named `foo`, and retrieves the value stored by the assignment expression.

The problem here is that, at compile-time, since the variable expression at line
2 only knows of the variable identifier, and the global runtime store is not
available at compile-time, there is no way to infer what value kind the variable
foo will resolve to at runtime.

To solve this problem we'll introduce a **compile-time store** to keep track of
resolve kinds for targets (variables and paths):

```rust
struct CompilerState {
    variables: HashMap<String, ResolveKind>,

    // …
}
```

We'll also update the `resolves_to` function to take a reference to
`CompilerState`.

```rust
pub trait Expression: Send + Sync {
    fn resolves_to(&self, store: &CompilerState) -> ResolveKind;
}
```

This allows (as an example) the `Variable` expression to fetch the resolve kind
of a variable:

```rust
impl Expression for Variable {
    fn resolves_to(&self, state: &CompilerState) -> ResolveKind {
        state.variable(&self.ident).unwrap_or(ResolveKind::Any)
    }
}
```

The `Assignment` expression will store the resolve kind of a variable on
initialization:

```rust
impl Assignment {
    fn new(ident: String, expression: Box<dyn Expression>, state: &mut CompilerState) -> Self {
        let resolve = expression.resolves_to(&state);
        state.variables_mut().insert(ident, resolve);

        Self { ident, expression }
    }
}
```

The same principle applies to query paths.

After this change, using the original example:

```rust
$foo = true
$foo
```

The compiler now knows that the variable `$foo` always resolves to a `Boolean`
kind.

## Doc-level Proposal

If a script is used in a Remap function within a `Condition`, the Remap program
is expected to return a `Maybe(Boolean)` resolve kind. A `Some(true)` value
passes the condition, `Some(false)` and `None` fail the condition.

Any other value expectation is a boot-time error, which can be solved among
other ways by explicitly checking for `… == …` (e.g. using the `Equality`
expression), or using a function such as `to_bool`.

## Rationale

The believe is that the proposed implementation is straight-forward enough, and
causes a minimal amount of churn, that the advantages (more boot-time checks)
outweigh the cost - adding more complexity to the language **implementation**
(there is no change in the usage or syntax of the language).

## Drawbacks

The biggest drawback is that each expression (and thus each new function) has to
implement the new `resolves_to` method, which increases work needed to add new
functions to the language.

Other than that, the usual drawback of maintaining the code we write applies
here.

## Alternatives

### Keep Things As They Are

We can forgo adding a compile-time check on the return value of a program. This
means we can't guide users how they should use the language within a given
context, requiring them to check their Vector log output to know when a program
fails.

The biggest downside to this is that the program can fail for one event, but not
for another, so making sure things work for the first x events that are
processed is not enough, you need to add monitoring to be aware of any script
failures for events that didn't return the expected value kind for a given event
path.

### Force Specific Final Expression

An alternative solution is to force a program to use a specific final expression
implementation (or one of several).

For example, if we want a program to always return a boolean, we can require the
final expression to be one of `Equality`, `Comparison`, or the `to_bool`
function.

The problem with this is:

1. It puts continuous burden on us to keep those program call-sites accurate. If
   we add a new `my_func` function which also always resolves to a boolean, it
   must be explicitly allowed to work within a given context.

2. Because functions are provided when a program is compiled (the Remap language
   itself does not contain any functions), there are no guarantees that
   `to_bool` is available, which makes this more tedious to get right.

### Implicitly Convert Final Expression

A program could be allowed to convert the final expression as it sees fit
(through some kind of function that takes an `Expression` and returns a
`Value`).

It could then match on whatever `Value` the final expression resolved to, and do
whatever it needs to, to convert that to the acceptable value kind (such as a
boolean) before returning the final value of the program.

This is probably the easiest to implement, but it will manipulate the outcome of
a program without the knowledge of the user, which can result in unexpected
situations and can still result in runtime failures if whatever value the final
expression returns cannot be converted to whatever value the program is expected
to resolve to.

## Outstanding Questions

- None, pending review feedback.

## Plan Of Attack

- [ ] Push initial proof-of-concept demonstration of proposed solution.
- [ ] Convert all existing expressions to implement `resolves_to`.
- [ ] Update `Condition` Remap usage to expect programs to return a `Boolean`
      value.
