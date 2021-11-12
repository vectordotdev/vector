# RFC 9811 - 2021-10-14 - VRL performance

VRL is currently a perfornce bottleneck in Vector. There are a number of
potential avenues for us to explore in order to optimise VRL.

## Context

https://github.com/vectordotdev/vector/issues/6680

https://github.com/vectordotdev/vector/issues/6770

## Scope

### In scope

This RFC discussing a number of changes that can be made to the Vrl runtime to
enable it to process data faster.

### Out of scope

Out of scope are:

- improvements to the build time of VRL (mainly caused by lalrpop).
- changes to improve the functionality of VRL.
- Using a bytecode VM to execute VRL (this will come in a followup PR).

## Pain

### Allocations

A lot of performance is taken up by excessive allocations. It is likely that
reducing the number of allocations will produce the biggest performance gain
for VRL and thus this should be the main area of focus.

Where we have clones that we should try to avoid:

- At the start of the Remap transform. The entire event is cloned so we can
  roll back to it on error.
- In the interface between VRL and Vector. The object is cloned on both `get`
  and `set` operations.

Let's have a look at the life of the event `{ "foo": { "bar": 42 } }` in this
simple VRL source:

```coffee
  .zub = .foo
```

1. The whole event is cloned in order to create a backup in case the event is
   aborted. [here](https://github.com/vectordotdev/vector/blob/v0.17.0/src/transforms/remap.rs#L135)
2. Then we need to access `.foo`. The `.foo` field needs to be pulled out of
   the event and converted to a `vrl::Value`. [here](https://github.com/vectordotdev/vector/blob/v0.17.0/lib/vector-core/src/event/vrl_target.rs#L130)
   This actually involves two allocations.
   a. First the value is cloned.
   b. Next it is converted into a `vrl::Value`.
      [here](https://github.com/vectordotdev/vector/blob/v0.17.0/lib/vector-core/src/event/value.rs#L308)
      Since `.foo` in at Object, this involves allocating a new `BTreeMap` then
      looping over each field, allocating a new `vrl::Value` for each one.
3. We then assign it to `.zub`. To do this the value is cloned and inserted
   into our event. [here](https://github.com/vectordotdev/vector/blob/v0.17.0/lib/vrl/compiler/src/expression/assignment.rs#L369)

Each time an object is passed between Vector and VRL an allocation is made.

### Boxing

VRL works by compiling the source to a tree based AST (abstract syntax tree).
To execute the program the tree is walked, each node is evaluated with the
current state of the program. Each node is not necessarily stored in memory
close to the next node. As a result it does not fully utilise the cpu cache.

Also, each node in the AST is boxed which causes extra pointer indirection.


## Proposal

### Implementation

### Reducing allocations

#### Use reference counting

One of the data types that is cloned the most is `vrl::Value`. A lot of these
clones are because VRL needs to maintain multiple references to a single value.
In the Rust model, this can get highly awkward handling the lifetimes.

If VRL used reference counting instead, the allocations can be avoided. Each
clone would just increase a count into the underlying data store which is
significantly cheaper. When the value goes out of scope the count is reduced.
If the count reaches 0, the memory is freed.

VRL needs to wrap the data in an `Rc`. This does mean pulling the data out of
the original event, which is not wrapped in `Rc`. Currently the data is cloned
at the start of the transform. Since this replaces the need for that clone the
additional cost required will be minimal - just an additional allocation
required for each part of the value to cater for the reference count.

Each field in the event can be pulled out lazily as it is used.

At the end of the process any modified data can be reapplied to the event. This
should be possible with a simple `mem::swap`. If the process is aborted, we
don't do anything since the original event is still unmodified.

Currently, the only point at which data is mutated is when updating the event.
With this change mutation would occur when updating the VRL data store.

Some changes will need to be made:

1. Change `vrl::Value::Object` to wrap the Value in `Rc<RefCell<>>`:
   `Object(BTreeMap<String, Rc<RefCell<Value>>>)`
2. Change `vrl::Value::Array`  to wrap the Value in `Rc<RefCell<>>`:
   `Array(Vec<Rc<RefCell<Value>>>)`
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

4. Change `VrlTarget`, to store the event as a `<Rc<RefCell<vrl_core::Value>>`
   rather than a `Value`.  This does mean that when the remap transform runs
   all the data from `Value` will need converting to `vrl_core::Value`. There
   is a cost involved here. However it does mean that we can avoid the initial
   clone that is often necessary, which should soak up most of the extra
   expense.
5. Update everything to use `Rc<RefCell<Value>>`. There is a risk here since
   using `RefCell` moves the borrow checking to the runtime rather than compile
   time. Affected areas of code will need to be very vigorously tested to avoid
   runtime panics. Fortunately a lot of Vrl relies on creating new values
   rather than mutating existing, so there isn't too much code that will be
   affected.
6. `Value` is no longer `Send + Sync`. There are some nodes in the AST that
   need to store a `Value`, for example `Variable` stores a `Value`.  We need
   to identify if this `Value` is actually necessary (it's not clear to me).
   If it isn't we can remove it.
   If it is we need to create a thread safe variable of `Value` that can be
   stored here and then converted into a `Value` at runtime.

Initial experiments roughly showed a reduction from 1m20s to 1m02s to push
100,000 records through Vector using reference counting.

### Flow analysis

One of the expensive operations is path lookup.

There may be certain situations where this can be improved by analyising the
flow of data at compile time.

For example, with this code:

```coffee
.thing[3].thang = 3
log(.thing[3].thang)
```

the compiler can work out that the variable written to and read from is the
same and can thus store that data in an array, any access can be made via a
quick O(1) lookup using the index to the array.

The flow analysis can become fairly complex.

```coffee
.message = parse_json!(.message)
.message[3] = parse_json!(.message[2])
log(.message[3].thing)
```

Functions like `parse_json` would need access to this array and hints as to
what paths should be stored.

A lot more thought needs to go into this before we can consider implementing
it.

#### Use a Bump Allocator?

We could use a library such as [bumpalo](https://crates.io/crates/bumpalo).

A bump allocator will allocate memory up front. This memory will then be used
during the runtime. At the end of each transform the bump pointer is reset to
the start - effectively deallocating all the memory used during the transform
with very little cost.

The downside to a bump allocator is we need to make sure sufficient memory is
allocated up front.

This is probably not ready yet until this PR for `BTreeMap` lands.
https://github.com/rust-lang/rust/pull/77438

### Optimizing stdlib functions

There is a small number of functions in the stdlib that follow a pattern of
converting `Bytes` into a `Cow<str>` using `String::from_utf8_lossy`. The
string is then allocated using `.into_owned()`.

With some shifting around it should be possible to avoid this extra allocation,
possibly at the cost of a slight reduction in code ergonomics.

The functions that could possibly be optimised are:

`contains`
`ends_with`
`starts_with`
`join`
`truncate`

## Rationale

Vrl is often identified as a bottleneck in performance tests. Being a key
component in Vector any optimisation that can be done in Vrl will benefit
Vector as a whole.

## Drawbacks

Downsides to moving Vrl to use reference counting:

- Using `RefCell` does move the borrow checking to the runtime. Without compile
  time checks the chances of a panic are much higher.

## Alternatives


### References

This RFC proposes using Reference Counted Values. However it is possible that
we can use pure references. This would save the overhead of having to maintain
a reference count using `Rc`. The data would be owned by the `Target`.  The
signature for `get` would become:

```rust
    fn get(&'a self, path: &LookupBuf) -> Result<Option<&'a Value>, String>;
```

In theory, this change would be more performant than reference counting. However,
the extra lifetimes could cause sufficient problems as to make this solution
unworkable.  We need to work on a spike to see what issues may arise from this
approach before determining the way forward.

## Outstanding Questions

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues
after the RFC is approved:

- [ ] Submit a PR with spike-level code _roughly_ demonstrating the change for
      referencing counting. See [here](https://github.com/vectordotdev/vector/pull/9785).
- [ ] Submit a PR with spike-level code _roughly_ demonstrating the change for
      references.
- [ ] Optimise the relevant stdlib functions.


## Future Improvements

Compiler technology can be a very advanced area. A lot of research has gone
into the area which can be leveraged in improving Vrl. As we look more into the
topic more and more ideas for areas of improvement will surface.
