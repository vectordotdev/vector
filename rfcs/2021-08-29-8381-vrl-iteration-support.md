# RFC 8381 - 2021-08-22 - VRL Iteration Support

We add native, limited support for iteration to VRL in a way that fits the VRL
[design document][doc], to allow operators to optimally remap their data.

## Table Of Contents

<!-- vim-markdown-toc GFM -->

* [Context](#context)
* [Cross cutting concerns](#cross-cutting-concerns)
* [Scope](#scope)
  * [In scope](#in-scope)
  * [Out of scope](#out-of-scope)
* [Pain](#pain)
* [Proposal](#proposal)
  * [User Experience](#user-experience)
    * [Example Use-Case](#example-use-case)
    * [Object Iteration](#object-iteration)
    * [Array Iteration](#array-iteration)
  * [Implementation](#implementation)
    * [Closure-support For Functions](#closure-support-for-functions)
    * [Lexical Scoping](#lexical-scoping)
    * [Returning Two-Element Array](#returning-two-element-array)
      * [Tuple Type](#tuple-type)
    * [Parser Changes](#parser-changes)
    * [Compiler Changes](#compiler-changes)
    * [Function Trait](#function-trait)
    * [Expression Trait](#expression-trait)
* [Rationale](#rationale)
* [Drawbacks](#drawbacks)
* [Prior Art](#prior-art)
* [Alternatives](#alternatives)
  * [For-Loop](#for-loop)
* [Outstanding Questions](#outstanding-questions)
* [Plan Of Attack](#plan-of-attack)
* [Future Improvements](#future-improvements)
  * [Iteration Control-Flow](#iteration-control-flow)
  * [Specialized Iteration Functions](#specialized-iteration-functions)
  * [Schema Support](#schema-support)
  * [Pipeline Operator Support](#pipeline-operator-support)
  * [Dynamic Field Assignment Support](#dynamic-field-assignment-support)

<!-- vim-markdown-toc -->

## Context

* Magic *_keys and*_values Remap functions [#5785][]
* feat(remap): add for-loop statement [#5875][]
* Remap enumerating/looping RFC [#6031][]

[#5785]: https://github.com/timberio/vector/issues/5785
[#5875]: https://github.com/timberio/vector/issues/5875
[#6031]: https://github.com/timberio/vector/issues/6031

## Cross cutting concerns

* New replace_keys Remap function [#5377][]
* New replace_values Remap function [#5783][]
* New redact_values Remap function [#5784][]
* Complex nested parsing with Remap (waninfo) [#5852][]
* enhancement(vrl): add filter_array [#7908][]

[#5377]: https://github.com/timberio/vector/issues/5377
[#5783]: https://github.com/timberio/vector/issues/5783
[#5784]: https://github.com/timberio/vector/issues/5784
[#5852]: https://github.com/timberio/vector/issues/5852
[#7908]: https://github.com/timberio/vector/issues/7908

## Scope

### In scope

* Ability to iterate/map over objects and arrays.
* Additional language constructs to support iteration.

### Out of scope

* Specialized forms of iteration (reduce, filter, etc...).
* Iterating any types other than objects or arrays.
* Iteration control-flow (e.g. `break` or `return`)
* Boundless iteration (e.g. `loop`).

## Pain

VRL is used to remap events to their desired state. Remapping involves
manipulating existing fields, or adding new ones.

One gap in the language right now is the possibility to _dynamically remap
fields_. That is, an event might have fields that can't be known at
compile-time, which you still want to manipulate.

To do this, you have to be able to _iterate_ over key/value pairs of your
object, and remap them individually. This requires some form of iteration
support in the language.

## Proposal

### User Experience

Operators gain access to a new `map` function that allows them to iterate over
objects or arrays, and manipulate key/value pairs.

The function takes a value, an optional "recursive" flag, and a closure to apply
to each individual key/value pair in an object or index/value pair in an array.

There is no unbounded `loop` iterator, to avoid accidental infinite loops in
programs. Additionally, control-flow statements (e.g. `break` or `return`) to
manipulate the iteration is not supported at this time (see "[future
improvements](#future-improvements)"). Iteration always runs to completion.

#### Example Use-Case

Let's take a look at this function in action:

```json
{
    "tags": {
        "foo": true,
        "bar": false,
        "baz": "no",
        "qux": [true, false],
        "quux": {
            "one": true,
            "two": false
        }
    },
    "ips": [
        "180.14.129.174",
        "31.73.200.120",
        "82.35.219.252",
        "113.58.218.2",
        "32.85.172.216"
    ]
}
```

```coffee
# Once "schema support" lands, this can be removed.
.tags = object(.tags) ?? {}
.ips = array(.ips) ?? []

# Recursively map all `.tags` to their new values.
#
# A copy of the object is returned, with the key/value changes applied.
.tags = map(.tags, recursive: true) { |tag, value|
    # `value` can be a boolean, or any other value. We enforce it to be
    # a boolean.
    value = bool!(value) ?? false

    # Manipulate the field string ("tag") if the value returns `true`.
    if value {
        tag = "__" + upcase(tag)
    }

    # Mapping an object requires you to return a two-element array, the first
    # being the string to which the key is set, the second the value of the
    # record.
    #
    # This invariant will be checked at compile-time.
    [tag, value]
}

# Map all IP addresses in `.ips`.
.ips = map(.ips) { |index, ip|
    # Enforce `ip` to be a string.
    ip = string(ip) ?? "unknown"

    # Mapping an array requires you to return a single value to which the
    # item-under-iteration will be mapped to.
    {
      "address": ip,
      "order": index,
      "private": starts_with(ip, "180.14"),
    }
}
```

```json
{
    "tags": {
        "__FOO": true,
        "bar": false,
        "baz": false,
        "qux": false,
        "quux": {
            "__ONE": true,
            "two": false
        }
    },
    "ips": [
        { "address": "180.14.129.174", "order": 0, "private": true },
        { "address": "31.73.200.120", "order": 1, "private": false },
        { "address": "82.35.219.252", "order": 2, "private": false },
        { "address": "113.58.218.2", "order": 3, "private": false },
        { "address": "32.85.172.216", "order": 4, "private": false }
    ]
}
```

#### Object Iteration

Let's start by looking at the function signature when iterating over an object
(the same function can be used for array iteration, which is explained down
below, but to keep the signature simple at first, we'll start with objects):

```coffee
map(value: OBJECT, recursive: BOOLEAN) { |<key variable>, <value variable>| [EXPRESSION, EXPRESSION] } -> OBJECT
```

Let's break this down:

* The function name is `map`.
* It takes two arguments, `value` and `recursive`.
  * `value` has to be of type `object`, which is the object to be iterated over.
  * `recursive` has to be of type `boolean`, determining whether to iterate over
    nested objects (_not_ arrays). It defaults to `false`.
* A closure-like expression is expected as part of the function call, but after
  the closing `)`.
  * This takes the form of `{ |...| expression }`.
  * When iterating over an object, `|...|` has to represent two variables, one
    for the key, and one for the value (f.e. `|key, value|`).
  * The expression has to return a 2-element `array`
  * the first element is the new `key` value, the second the `value` value
* The function returns a new `object`, with the manipulated keys/values.

Here's a simplified example on how to use the function:

```json
{ "foo": true, "bar": false }
```

```coffee
. = map(.) { |key, value|
    key = upcase(key)
    value = !value

    [key, value]
}
```

```json
{ "FOO": false, "BAR": true }
```

As an example, the shortest form to write the above example in would be:

```coffee
. = map(.) { |k,v| [upcase(k), !v] }
```

The object under iteration is not mutated, instead a copy of the value is
iterated, and mutated, returning a new object or array after iteration
completes.

#### Array Iteration

The signature for array iteration is as follows:

```coffee
map(value: ARRAY, recursive: BOOLEAN) { |<index variable>, <value variable>| EXPRESSION } -> ARRAY
```

This is nearly identical to the object signature, except that it takes an array,
has a variable for the index of the current item, and returns a single
expression to use as the value at the given index. Additionally, the `recursive`
flag only recurses into nested arrays, _not_ objects.

Here's an example:

```json
["foo", "bar"]
```

```coffee
. = map(.) { |index, value|
    value + "_" + to_string!(index)
}
```

```json
["foo_0", "bar_1"]
```

### Implementation

This proposal favors adding a _mapping_ function over _for-loop syntax_. That
is, the RFC proposes:

```coffee
map(.) { |key, value| [upcase(key), value] }
```

over:

```coffee
for (key, _value) in . {
  key = upcase(key)
}
```

This choice is made both on technical merits, based on the [VRL Design
Document][doc] and for improved future capabilities. See the
"[for-loop](#for-loop)" alternative section for more details on this.

For the chosen proposal to work, there are two separate concepts that need to
be implemented:

* closure-support for functions
* lexical scoping

Let's discuss these one by one, before we arrive at the final part, implementing
the `map` function that uses both concepts.

[doc]: https://github.com/timberio/vector/blob/jean/vrl-design-doc/lib/vrl/DESIGN.md

#### Closure-support For Functions

For iteration to land in the form proposed in this RFC, we need a way for
operators to write _what_ they want to happen to keys and/or values of objects
and arrays.

We do this by allowing functions to expose the fact that they accept a closure
as a stand-alone argument to their function call.

"stand-alone" means the closure comes _after_ the function call itself, e.g.
this:

```coffee
map(.) { |k, v|
  [k, v]
}
```

over this:

```coffee
map(., { |k, v|
  [k, v]
})
```

This choice is made to make it clear that closures in VRL _can't be passed
around through variables, but are instead syntactically attached to a function
call_.

That is, we don't want to allow this:

```coffee
my_closure = { |k, v| [k, v] }
map(., my_closure)
```

There are several reasons for rejecting this functionality:

* It allows for slow or infinite recursion, violating the "Safety and
  performance over ease of use" VRL design principle.

* It can make reading (and writing) VRL programs more complex, and code can no
  longer be reasoned about by reading from top-to-bottom, violating the "design
  the feature for the intended target audience" design principle.

* We cannot allow assigning closures to event fields, requiring us to make
  a distinction between assigning to a _variable_ and an _event field_, one we
  haven't had to make before, and would like to avoid making.

* In practice, we haven't seen any use-case from operators that couldn't be
  solved by the current RFC proposal, but would be solved by the above syntax.

Instead, the closure-syntax is tied to a function call, and can only be added to
functions that explicitly expose their ability to take a closure with `x`
arguments that returns `y` value.

The return type of a closure is checked at compile-time, including the
requirement in `map` for a two-element array.

The variable names used to access the provided closure values (e.g. `|key,
value|`) are checked at compile-time to make sure you are actually using the
variables (to avoid potential variable name typo's). This behaves the same to
any other "unused variable assignment" checks happening at compile-time.

#### Lexical Scoping

Lexical scoping (variables being accessible within a given scope, instead of
globally) is something we've discussed before.

Before, we decided that the complexity of adding lexical scoping wasn't worth
the investment before our first release, and we also hoped that lexical scoping
wouldn't be something that was ever needed in VRL.

With this feature, and particular the function-closure syntax, lexical scoping
comes to top of mind again.

The reason for that, is the following example:

```coffee
map(.) { |key, value|
  key = upcase(key)

  [key, value]
}

key
```

We reference `key` outside the closure, at the last line of the program. What
should the value of `key` be in this case?

Without lexical scoping, it would be set to the upper-case variant of the "last"
key in the event.

With lexical scoping, it would return an "undefined variable" error at
compile-time, because the `key` variable _inside_ the closure is
lexically-scoped to that block, and remains undefined outside of the block.

However, while the above syntax would be _new_ and thus not a breaking change,
for existing code, adding lexical scoping _would_ be a breaking change:

```coffee
{
  foo = "baz"
}

foo
```

Previously, `foo` would return `"baz"` when the program runs, but with lexical
scoping, the compiler returns an "undefined variable" compilation error instead.

This is a breaking change, but because it results in a compilation error, there
will not be any unexpected runtime behavior for this case.

There is one additional case that _will_ result in a change in runtime behavior:

```coffee
foo = "bar"

{
  foo = "baz"
}

foo
```

Previously, `foo` would be set to `"baz"`, while lexical scoping means `foo`
outside the block will stay at `"bar"`. While we'll treat this as a breaking
change, we consider this pattern unlikely to be present in production code, and
so we accept this change in VRL.

In terms of exact rules, the following applies to lexical scoping in VRL:

* A VRL program has a single "root" scope, to which any unnested code belongs.
* A new scope is created by using the block (`{ ... }`) expression.
* Nested block expressions result in nested scopes.
* Function-closures also create a new scope.
* Any variable defined in a higher-level scope is accessible in nested scopes.
* Any variable defined in a lower-level scope _cannot_ be accessed in parent
  scopes.
* If a variable with the same identifier is overwritten in a lower-level scope,
  higher-level scopes will keep access to the original value assigned to that
  variable.

#### Returning Two-Element Array

We require the `map` function closure to return a two-element `array` type.
Without this requirement, mapping would work as follows:

```coffee
map(.) { |key, _|
  key = upcase(key)
}
```

That is, `key` would be a "special variable" inside the closure, which modifies
the actual key of the record within the object.

This doesn't fit existing patterns in VRL. It looks as if there's a _dangling_
variable `key` at the end that remains unused, but because we special-cased this
situation, it would instead magically update the actual key in the object after
the closure runs to completion.

This can become more difficult to reason about if/when we introduce control-flow
statements such as `break`, as you could have set `key` before calling `break`,
which would then either still mutate the actual key, or not, depending on how we
implement `break`, but either way, the program itself becomes less readable, and
operators have to read the language documentation to understand the semantic
differences between how code behaves _inside_ a function-closure and _outside_.

Instead, the `map` function-closure is required to return a two-element array of
`[key, value]`, which the function machinery then uses to update the actual
values of the object record, e.g.:

```coffee
map(.) { |key, value|
  key = upcase(key)

  # The array return-value clearly defines the eventual key and value values.
  [key, value]
}
```

##### Tuple Type

Alternatively, we could introduce a new `tuple` type to define the return-type
of the closure:

```coffee
map(.) { |key, value|
  key = upcase(key)

  (key, value)
}
```

They would semantically be the same, given that VRL supports mixing value types
in arrays, which is usually the difference between a tuple and an array, that,
and the fact that a tuple is immutable, but an array doesn't have to be.

Semantically, using a tuple makes more sense, but it does add an extra `Value`
type, and it does mean we have to convert that tuple to a type supported by
JSON, which will likely be an array, so the external JSON representation of
tuples remains the same, regardless of whether we use arrays or tuples inside
VRL itself.

Since there isn't a clear benefit at this moment to using a tuple over
a two-element array, the choice is made to forgo adding the tuple type at this
moment.

#### Parser Changes

Because the closure syntax will be tied to function calls, we don't need to add
a new top-level node type to the abstract syntax tree (AST). Instead, we need to
extend the existing `FunctionCall` type to support an optional closure:

```rust
pub struct FunctionCall {
    pub ident: Node<Ident>,
    pub abort_on_error: bool,
    pub arguments: Vec<Node<FunctionArgument>>,
}
```

We'll modify the type to this:

```rust
pub struct FunctionCall {
    pub ident: Ident,
    pub abort_on_error: bool,
    pub arguments: Vec<FunctionArgument>,
    pub closure: Option<FunctionClosure>,
}

pub struct FunctionClosure {
    pub variables: Vec<Ident>,
    pub block: Block,
}
```

Next, we need to teach the parser to parse optional closures for function calls.

The existing [LALRPOP][] grammar:

```rust
FunctionCall: FunctionCall = {
    <ident: Sp<"function call">> <abort_on_error: "!"?> "("
        NonterminalNewline*
        <arguments: CommaMultiline<Sp<FunctionArgument>>?>
    ")" => { /* ... */ },
};
```

Is updated to support optional closures:

```rust
FunctionCall: FunctionCall = {
    <ident: Sp<"function call">> <abort_on_error: "!"?> "("
        NonterminalNewline*
        <arguments: CommaMultiline<Sp<FunctionArgument>>?>
    ")" <closure: FunctionClosure?> => { /* ... */ },
};

#[inline]
FunctionClosure: FunctionClosure = {
    "{"
      "|" <variables: CommaList<"identifier">?> "|" NonterminalNewline*
      <expressions: Exprs>
    "}" => FunctionClosure { variables, block: Block(expressions) },
};
```

This will allow the parser to unambiguously parse optional function closures,
and add them as nodes to the program AST.

[lalrpop]: https://lalrpop.github.io/lalrpop/

#### Compiler Changes

Once the parser knows how to parse function closures, the compiler needs to
interpret them.

To start, we need to update the `FunctionCall` expression:

```rust
pub struct FunctionCall {
    expr: Box<dyn Expression>,
    abort_on_error: bool,
    maybe_fallible_arguments: bool,

    // new addition
    closure: Option<FunctionClosure>,
}

pub struct FunctionClosure {
    variables: Vec<dyn Expression>,
    block: Block,
}
```

We also need to update `compile_function_call` (not expanded here), to translate
the AST to updated `FunctionCall` expression type.

#### Function Trait

The bulk of the work needs to happen in the `Function` trait:

```rust
pub type Compiled = Result<Box<dyn Expression>, Box<dyn DiagnosticError>>;

pub trait Function: Sync + fmt::Debug {
    /// The identifier by which the function can be called.
    fn identifier(&self) -> &'static str;

    /// One or more examples demonstrating usage of the function in VRL source
    /// code.
    fn examples(&self) -> &'static [Example];

    /// Compile a [`Function`] into a type that can be resolved to an
    /// [`Expression`].
    ///
    /// This function is called at compile-time for any `Function` used in the
    /// program.
    ///
    /// At runtime, the `Expression` returned by this function is executed and
    /// resolved to its final [`Value`].
    fn compile(&self, state: &super::State, arguments: ArgumentList) -> Compiled;

    /// An optional list of parameters the function accepts.
    ///
    /// This list is used at compile-time to check function arity, keyword names
    /// and argument type definition.
    fn parameters(&self) -> &'static [Parameter] {
        &[]
    }
}
```

First, we're going to have to extend the `compile` method to take an optional
`Closure`:

```rust
fn compile(&self, state: &super::State, arguments: ArgumentList, closure: Option<FunctionClosure>) -> Compiled;
```

This will require us to update all currently existing function implementations,
but this is a mechanical change, as no existing functions can deal with closures
right now, so all of them will add `_closure: Option<Closure>` to their method
implementation, to indicate to the reader/Rust compiler that the closure
variable is unused.

Next, we need to have a way for the function definition to tell the compiler
a few questions:

1. Does this function accept a closure?
2. If it does, how many variable names does it accept?
3. What type will the variables have at runtime?
4. What return type must the closure resolve to?

To resolve these questions, function definitions must implement a new method:

```rust
fn closure(&self) -> Option<closure::Definition> {
    None
}
```

With `closure::Definition` defined as such:

```rust
mod closure {
    /// The definition of a function-closure block a function expects to
    /// receive.
    struct Definition {
        inputs: Vec<Input>,
    }

    /// One input variant for a function-closure.
    ///
    /// A closure can support different variable input shapes, depending on the
    /// type of a given parameter of the function.
    ///
    /// For example, the `map` function takes either an `Object` or an `Array`
    /// for the `value` parameter, and the closure it takes either accepts
    /// `|key, value|`, where "key" is always a string, or `|index, value|` where
    /// "index" is always a number, depending on the parameter input type.
    struct Input {
        /// The parameter name upon which this closure input variant depends on.
        parameter: &'static str,

        /// The value kind this closure input expects from the parameter.
        kind: value::Kind,

        /// The list of variables attached to this closure input type.
        variables: Vec<Variable>,

        /// The return type this input variant expects the closure to have.
        output: Output,
    }

    /// One variable input for a closure.
    ///
    /// For example, in `{ |foo, bar| ... }`, `foo` and `bar` are each
    /// a `ClosureVariable`.
    struct Variable {
        /// The value kind this variable will return when called.
        kind: value::Kind,
    }

    enum Output {
        Array {
            /// The number, and kind of elements expected.
            elements: Vec<value::Kind>,
        }

        Object {
            /// The field names, and value kinds expected.
            fields: HashMap<&'static str, value::Kind,
        }

        Scalar {
            /// The expected scalar kind.
            kind: value::Kind,
        }

        Any,
    }
}
```

As shown above, the default trait implementation for this new method returns
`None`, which means any function (the vast majority) that doesn't accept
a closure can forgo implementing this method, and continue to work as normal.

In the case of the `map` function, we'd implement it like so:

```rust
fn closure(&self) -> Option<closure::Definition> {
    let field = closure::Variable { kind: kind::String };
    let index = closure::Variable { kind: kind::Integer };
    let value = closure::Variable { kind: kind::Any };

    let object = closure::Input {
        parameter: "value",
        kind: kind::Object,
        variables: vec![field, value],
        output: closure::Output::Array {
            elements: vec![kind::String, kind::Any],
        }
    };

    let array = closure::Input {
        parameter: "value",
        kind: kind::Array,
        variables: vec![index, value],
        output: closure::Output::Any,
    };

    Some(closure::Definition {
        inputs: vec![object, array],
    })
}
```

With the above in place, `map` can now iterate over both objects and arrays, and
depending on which type is detected at compile-time, the closure attached to the
function call can make guarantees about which type the first variable name will
have.

For example:

```coffee
. = { "foo": true }
. = map(.) { |key, value| [key, value] }
```

```coffee
. = ["foo", true]
. = map(.) { |_index, value| value }
```

In the first example, because the compiler knows `map` receives an object as its
first argument, it can guarantee that `key` will be a string, and `value` of
"any" type. Additionally, it can show a compile-time error if the last
expression in the block is not an array, with two elements, and the first
element being of the string kind.

The second example is similar, except that it accepts any return value, and
guarantees that the first variable is a number (the index of the value in the
array).

Note that for the above to work, the compiler must know the _exact_ type
provided to (in this case) the `value` function parameter. It can't be _either
array or object_, it has to be exactly one of the two. Operators can guarantee
this by using `to_object`, etc.

#### Expression Trait

With all of this in place, the `map` function can compile its expression given
the closure details, and run the closure multiple times to completion, doing something
like this:

```rust
fn resolve(&self, ctx: &mut Context) -> Result<Value, Error> {
    let run = |key, value| {
        // TODO: handle variable scope stack
        ctx.variables.insert(key, value);
        let closure_value = self.closure.resolve(self)?;
        ctx.variables.remove(key);

        Ok(closure_value)
    };

    let result = match self.value.resolve(ctx)? {
        Value::Object(object) => {
            let mut result = BTreeMap::default();

            for (key, value) in object.into_iter() {
                let v = run(key, value)?.try_array()?;
                result.insert(v[0], v[1]);
            }

            result.into()
        }
        Value::Array(array) => {
            let mut result = Vec::with_capacity(array.len());

            for (index, value) in array.into_iter().enumerate() {
                let v = run(index, value)?;
                result.push(v);
            }

            result.into()
        }
        _ => unreachable!("expected object or array"),
    };

    Ok(result)
}
```

This should get us most of the way towards adding function-closure support to
VRL, and using that support in the initial `map` function to do its work.

## Rationale

Iteration unlocks solutions to many remapping scenarios we currently don't
support. Not implementing this RFC would hold VRL back, and prevent operators
with more complex use-cases from using Vector with VRL to achieve their goals.

By adding iteration, we unlock the capability to resolve almost all use-cases in
the future by introducing more iteration-based functions.

## Drawbacks

* It adds more complexity to the language.
* There are potential performance foot guns when iterating over large
  collections.
* The parser and compiler have to become more complex to support this use-case.

## Prior Art

* [Rust `Iterator` trait](https://doc.rust-lang.org/std/iter/trait.Iterator.html#)
* [Nested data structure traversal examples](https://github.com/josevalim/nested-data-structure-traversal)
* [Ruby blocks](https://www.tutorialspoint.com/ruby/ruby_blocks.htm)
* [Rust closures](https://doc.rust-lang.org/book/ch13-01-closures.html)

## Alternatives

### For-Loop

A different approach to iteration is to use a built-in syntax for-loop:

```coffee
for (key, _value) in . {
  key = upcase(key)
}
```

The biggest strength of this approach is the simplicity of the syntax, and the
familiarity with many other languages that have for-loops.

It's relevant to mention that this solution also still needs lexical-scoping
implemented, to avoid "leaking" the values of the `key` and `value` variables
outside of the loop.

One problem with this approach is that recursive iteration (accessing nested
object fields) isn't possible, unless we add another special syntax (e.g.
`recursive for (.., ..) in . {}`). This adds more surface-level syntax and
removes some of its familiarity, making it a less attractive solution.

An additional problem is that the `key` and `value` variables become "special",
in that, even though it _appears_ that they aren't used after assignment, the
`for-loop` expression would actually update the object key after each iteration
in the loop.

While this is technically the same problem we had to solve in the function-based
solution, applying that same solution to a `for-loop` again makes it look less
like for-loops in other languages, defeating one of the strengths of this
approach:

```coffee
for (key, value) in . {
  key = upcase(key)

  (key, value)
}
```

A solution to the magic-variable problem would be to allow dynamic paths, and
have operators directly assign to those paths:

```coffee
for (key, _value) in . {
  .[upcase(key)] = value
}
```

This solves one problem, but introduces another: using `.<path>` always starts
at the root of the target. Given the following example:

```json
{ "foo": { "bar": true } }
```

How would we use dynamic paths in a recursive for-loop?

```coffee
recursive for (key, value) in . {
  .[upcase(key)] = value
}
```

Because key is `"foo"` and then `"bar"`, you would end up with:

```json
{ "FOO": true, "BAR": true }
```

Which is not the expected outcome.

This could be solved by making `.` relative in the for-loop, but that's a major
shift from the current way VRL works, requires a new way of accessing the root
object if you can't use `.`, and goes against the rules as laid out in the
[design document][doc].

---

* What other approaches have been considered and why did you not choose them?
* How about not doing this at all?

## Outstanding Questions

* Do we want to introduce any form of lexical-scoping in this RFC, or keep the
  status-quo for now?
* Do we want to introduce tuple-expressions or are we satisfied with using
  a two-element array as the return value for `map`?
* ...

## Plan Of Attack

* [ ] Add lexical scoping to VRL
* [ ] Add support for parsing function-closure syntax
* [ ] Add support for compiling function-closure syntax
* [ ] Add new `map` function
* [ ] Document new functionality

## Future Improvements

### Iteration Control-Flow

While likely desirable, this RFC intentionally avoids control-flow operations
inside iterators.

They are likely to be one of the first enhancements to this feature, though:

```coffee
. = map(.) |key, value| {
  # Return default key/value pairs if the value is an object.
  if is_object(value) {
    return [key, value]
  }

  # ...
}
```

### Specialized Iteration Functions

Once this RFC is implemented, additional iteration capability can be expanded by
adding new functions to the standard library.

For example, filtering:

```coffee
# Return a new array with "180.14.129.174" removed.
.ips = filter(.ips) |_index, ip| {
    ip = string(ip) ?? "unknown"

    !starts_with(ip, "180.14")
}
```

Or ensuring all elements adhere to a condition:

```coffee
# Add new `all_public` boolean field.
.all_public = all(.ips) |_index, ip| {
    ip = string(ip) ?? "unknown"

    !starts_with(ip, "180.14")
}
```

Some additional suggestions include `flatten`, `partition`, `fold`, `any`,
`find`, `max`, `min`, etc...

### Schema Support

Once [schema support][] lands, writing iterators can become less verbose.

For example, this example from the RFC:

```coffee
.ips = array(.ips) ?? []
.ips = filter(.ips) |_index, ip| {
    ip = string(ip) ?? "unknown"

    !starts_with(ip, "180.14")
}
```

Can be written as follows, when applying the correct schema:

```coffee
.ips = filter(.ips) |_, ip| !starts_with(ip, "180.14")
```

Because a type schema could guarantee the compiler that `.ips` is an array, with
only string items.

### Pipeline Operator Support

Once the [pipeline operations][] land, we can further expand the above example
as follows:

```coffee
.private_and_public_ips = filter(.ip) |_, ip| is_ip(ip) |> partition() |_, ip| starts_with(ip, "180.14")
```

### Dynamic Field Assignment Support

Once [dynamic field assignment][] lands, you can dynamically move fields as
well:

```json
["foo", "bar", "baz"]
```

```coffee
for_each(.) |index, value| .[value] = index
```

```json
{
    "foo": 0,
    "bar": 1,
    "baz": 2
}
```
