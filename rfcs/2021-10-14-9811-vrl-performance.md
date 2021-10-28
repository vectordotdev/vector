# RFC 9811 - 2021-10-14 - VRL performance

VRL is currently a perfornce bottleneck in Vector. There are a number of potential avenues for us to explore
in order to optimise VRL.

## Context

https://github.com/vectordotdev/vector/issues/6680
https://github.com/vectordotdev/vector/issues/6770

## Scope

### In scope

This RFC discussing a number of changes that can be made to the Vrl runtime to enable it to process data faster.

### Out of scope

Out of scope are:

- improvements to the build time of Vrl (mainly caused by lalrpop).
- changes to improve the functionality of Vrl.

## Pain

### Allocations

A lot of performance is taken up by excessive allocations. It is likely that reducing the number of allocations
will produce the biggest performance gain for VRL and thus this should be the main area of focus.

Where we have clones that we should try to avoid:

- At the start of the Remap transform. The entire event is cloned so we can roll back to it on error.
- In the interface between VRL and Vector. The object is cloned on both `get` and `set` operations.

Let's have a look at the life of the event `{ "foo": { "bar": 42 } }` in this simple VRL source:

```coffee
  .zub = .foo
```

1. The whole event is cloned in order to create a backup in case the event is aborted.
   [here](https://github.com/vectordotdev/vector/blob/v0.17.0/src/transforms/remap.rs#L135)
2. Then we need to access `.foo`. The `.foo` field needs to be pulled out of the event and converted to a `vrl::Value`.
   [here](https://github.com/vectordotdev/vector/blob/v0.17.0/lib/vector-core/src/event/vrl_target.rs#L130)
   This actually involves two allocations.
   a. First the value is cloned.
   b. Next it is converted into a `vrl::Value`.
      [here](https://github.com/vectordotdev/vector/blob/v0.17.0/lib/vector-core/src/event/value.rs#L308)
      Since `.foo` in at Object, this involves allocating a new `BTreeMap` then looping over each field, allocating a
      new `vrl::Value` for each one.
3. We then assign it to `.zub`. To do this the value is cloned and inserted into our event.
   [here](https://github.com/vectordotdev/vector/blob/v0.17.0/lib/vrl/compiler/src/expression/assignment.rs#L369)

Each time an object is passed between Vector and VRL an allocation is made.

### Boxing

VRL works by compiling the source to a tree based AST (abstract syntax tree). To execute the program the tree is walked, each node
is evaluated with the current state of the program. Each node is not necessarily stored in memory close to the next node. As a result
it does not fully utilise the cpu cache.

Also, each node in the AST is boxed which causes extra pointer indirection.


## Proposal

### Implementation

### Reducing allocations

#### Use reference counting

One of the data types that is cloned the most is `vrl::Value`. A lot of these clones are because VRL needs to maintain multiple references to
a single value. In the Rust model, this can get highly awkward handling the lifetimes.

If VRL used reference counting instead, the allocations can be avoided. Each clone would just increase a count into the underlying data store
which is significantly cheaper. When the value goes out of scope the count is reduced. If the count reaches 0, the memory is freed.

VRL needs to wrap the data in an `Rc`. This does mean pulling the data out of the original event, which is not wrapped in `Rc`. Since we already
clone the data at the start of the transform this is not going to create any additional cost to what we already have.

Each field in the event can be pulled out lazily as it is used.

At the end of the process any modified data can be reapplied to the event. This should be possible with a simple `mem::swap`. If the process
is aborted, we don't do anything since the original event is still unmodified.

Currently, the only point at which data is mutated is when updating the event. With this change mutation would occur when updating the VRL data store.

Some changes will need to be made:

1. Change `vrl::Value::Object` to wrap the Value in `Rc<RefCell<>>`:  `Object(BTreeMap<String, Rc<RefCell<Value>>>)`
2. Change `vrl::Value::Array`  to wrap the Value in `Rc<RefCell<>>`: a`Array(Vec<Rc<RefCell<Value>>>)`
3. Change the methods of the `Target` trait to use `Rc<RefCell<>>`:

```rust
    fn insert(&mut self, path: &LookupBuf, value: Rc<RefCell<Value>>) -> Result<(), String>;
    fn get(&self, path: &LookupBuf) -> Result<Option<Rc<RefCell<Value>>>, String>;
    fn remove(
        &mut self,
        path: &LookupBuf,
        compact: bool,
    ) -> Result<Option<Rc<RefCell<Value>>>, String>;
```
4. Change `VrlTarget`, to store the event as a `<Rc<RefCell<vrl_core::Value>>` rather than a `Value`.
   This does mean that when the remap transform runs all the data from `Value` will need converting to `vrl_core::Value`. There
   is a cost involved here. However it does mean that we can avoid the initial clone that is often necessary, which should
   soak up most of the extra expense.
5. Update everything to use `Rc<RefCell<Value>>`. There is a risk here since using `RefCell` moves the borrow
   checking to the runtime rather than compile time. Affected areas of code will need to be very vigorously tested
   to avoid runtime panics. Fortunately a lot of Vrl relies on creating new values rather than mutating existing,
   so there isn't too much code that will be affected.
6. `Value` is no longer `Send + Sync`. There are some nodes in the AST that need to store a `Value`, for example `Variable`
   stores a `Value`.  We need to identify if this `Value` is actually necessary (it's not clear to me). If it isn't we can remove it.
   If it is we need to create a thread safe variable of `Value` that can be stored here and then converted into a `Value` at runtime.

Initial experiments roughly showed a reduction from 1m20s to 1m02s to push 100000 records through Vector using reference counting.

### Bytecode VM

Vrl compiles to an AST that is then walked during resolution. Each node in that tree is boxed and stored in disparate
regions of memory. As a result walking the tree means that the CPU caches must be constantly swapped.


Instead we can create a bytecode VM to store the execution of the Vrl program.

Bytecode is essentially a big enum of instructions:

```rust
#[derive(FromPrimitive, ToPrimitive, Copy, Clone, Debug, PartialEq, Eq)]
pub enum OpCode {
    Return = 255,
    Constant,
    Negate,
    Add,
    Subtract,
    Multiply,
    Divide,
    Print,
    Not,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    NotEqual,
    Equal,
    Pop,
    JumpIfFalse,
    Jump,
    SetPath,
    GetPath,
    Call,
    ...
}
```

The Vm is a struct comprising of the following fields:

```rust
#[derive(Clone, Debug, Default)]
pub struct Vm {
    instructions: Vec<usize>,
    values: Vec<Literal>,
    targets: Vec<Variable>,
    stack: Vec<Value>,
    ip: usize,
}
```

- instructions

The instructions field is a `Vec` of `OpCode` cast to a usize. The reason for the cast is because not all instructions
are `OpCode`. For example the instructions `[.., Constant, 12, ..]` when evaluated will load the constant stored in
the `values` `Vec` that is found in position 12 onto the stack.

- values

A list of constant values found in the program. Since the bytecode only contains integers any actual values must be stored
here. This also allows literals to be deduped.

- targets

A list of paths used in the program, similar to `values`.

- stack

The Vm is a stack based Vm. Every expression that is evaluated pushes the result on the stack. Every operation pulls
the values it uses from the stack.

- ip

The instruction pointer points to the next instruction to evaluate.

With each node of the AST compiled down to just a few bytes and all instructions held in contiguous memory evaluation
of the program should be able to take full advantage of the CPU cache which should result in much faster execution.


#### Calling functions

Calling functions in the stdlib will be a case of evaluating each parameter with the results pushed onto the stack.

Since Vrl allows for named parameters and optional parameters, the compiler will need to ensure the bytecode
evaluates each parameter in a specific order (likely the order declared in the function). Bytecode will need to be
emmited for parameters that are not specified in the Vrl script to add a default value to the stack.

### Flow analysis

One of the expensive operations is path lookup.

There may be certain situations where this can be improved by analyising the flow of data at compile time.

For example, with this code:

```
.thing[3].thang = 3
log(.thing[3].thang)
```

the compiler can work out that the variable written to and read from is the same and can thus store that data in an
array, any access can be made via a quick O(1) lookup using the index to the array.

The flow analysis can become fairly complex.

```
.message = parse_json!(.message)
.message[3] = parse_json!(.message[2])
log(.message[3].thing)
```

Functions like `parse_json` would need access to this array and hints as to what paths should be stored.

A lot more thought needs to go into this before we can consider implementing it.

#### Use a Bump Allocator?

We could use a library such as [bumpalo](https://crates.io/crates/bumpalo).

A bump allocator will allocated a significant amount of memory up front. This memory will then be used


Use a persistent data structure to avoid the initial clone.
Create an mutated pool

This is probably not ready yet until this PR for `BTreeMap` lands.
https://github.com/rust-lang/rust/pull/77438


### Optimization

With the code as a single dimension array of Bytecode, it could be possible to scan the code for patterns and reorganise
the Bytecode so it can run in a more optimal way.

A lot more thought and research needs to go into this before we can consider implementing these changes.

### Optimizing stdlib functions

There is a small number of functions in the stdlib that follow a pattern of converting `Bytes` into a `Cow<str>` using
`String::from_utf8_lossy`. The string is then allocated using `.into_owned()`.

With some shifting around it should be possible to avoid this extra allocation, possibly at the cost of a
slight reduction in code ergonomics.

The functions that could possibly be optimised are:

`contains`
`ends_with`
`starts_with`
`join`
`truncate`

## Rationale

Vrl is often identified as a bottleneck in performance tests. Being a key component in Vector any optimisation
that can be done in Vrl will benefit Vector as a whole.

## Drawbacks

Downsides to moving Vrl to use reference counting:
- Using `RefCell` does move the borrow checking to the runtime. Without compile time checks the chances of a panic
  are much higher.

Downsides to using a Vm:
- The code is much more complex. With an AST that we walk it is fairly apparent what the code will be doing at any
  point. With a Vm, this is not the case, it is harder to look at the instructions in the Vm and follow back to what
  part of the Vrl code is being evaluated. We will need to write some extensive debugging tools to allow for decent
  introspection into the Vm.
- We lose a lot of safety that we get from the Rust compiler. There will need to be significant fuzz testing to
  ensure that the code runs correctly under all circumstances.
- Currently each stdlib function is responsible for evaluating their own parameters. This allows parameters to be
  lazily evaluated. Most likely with a Vm, the parameters will need to be evaluated up front and the stack passed
  into the function. This could impact performance.

## Prior Art

- Goscript - An implementation of Go using a bytecode Vm, https://github.com/oxfeeefeee/goscript

## Alternatives

This RFC lists a number of improvements that can be made to Vrl, we could do all or just some of them. Each of the
changes will need to be done with a big focus on how much of an improvement is actually possible with each
technique. Only then will we be able to judge if the extra code complexity is worth the improvement.

If we don't do any, Vrl will continue to be a bottleneck. It is also possible that Vrl will continue to be a
bottleneck after these changes, but hopefully just not as significant.

## Outstanding Questions


## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Submit a PR with spike-level code _roughly_ demonstrating the change for referencing counting.
      See [here](https://github.com/vectordotdev/vector/pull/9785)
- [ ] Submit a PR with spike-level code _roughly_ demonstrating the change for the VM.
- [ ] Optimise the relevant stdlib functions.


## Future Improvements

Compiler technology can be a very advanced area. A lot of research has gone into the area which can be leveraged
in improving Vrl. As we look more into the topic more and more ideas for areas of improvement will surface.
