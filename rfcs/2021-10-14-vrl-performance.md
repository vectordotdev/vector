# RFC <issue#> - 2021-10-14 - VRL performance

VRL is currently a perfornce bottleneck in Vector. There are a number of potential avenues for us to explore
in order to optimise VRL.

## Context

- Link to any previous issues, RFCs, or briefs (do not repeat that context in this RFC).

## Cross cutting concerns

- Link to any ongoing or future work relevant to this change.

## Scope

### In scope

- List work being directly addressed with this RFC.

### Out of scope

- List work that is completely out of scope. Use this to keep discussions focused. Please note the "future changes" section at the bottom.

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

### User Experience

- Explain your change as if you were describing it to a Vector user. We should be able to share this section with a Vector user to solicit feedback.
- Does this change break backward compatibility? If so, what should users do to upgrade?

### Implementation

### Reducing allocations

#### Use reference counting

One of the data types that is cloned the most is `vrl::Value`. A lot of these clones are because VRL needs to maintain multiple references to
a single value. In the Rust model, this can get highly awkward handling the lifetimes.

If VRL used reference counting instead, the allocations can be avoided. Each clone would just increase a count into the underlying data store
which is significantly cheaper.

VRL needs to wrap the data in an `Rc`. This does mean pulling the data out of the original event, which is not wrapped in `Rc`. Since we already
clone the data at the start of the transform this is not going to create any additional cost to what we already have.

Each field in the event can be pulled out lazily as it is used.

At the end of the process any modified data can be reapplied to the event. This should be possible with a simple `mem::swap`. If the process
is aborted, we don't do anything since the original event is still unmodified.

Currently, the only point at which data is mutated is when updating the event. With this change mutation would occur when updating the VRL data store.


Some changes will need to be made:

1. `trait Expression` can no longer be `Send + Sync` since `Rc` is `!Send`

#### Use a Bump Allocator?

We could use a library such as [bumpalo](https://crates.io/crates/bumpalo).

A bump allocator will allocated a significant amount of memory up front. This memory will then be used


Use a persistent data structure to avoid the initial clone.
Create an mutated pool

This is probably not ready yet until this PR for `BTreeMap` lands.
https://github.com/rust-lang/rust/pull/77438


### Bytecode VM

### Optimization

With the


## Rationale

- Why is this change worth it?
- What is the impact of not doing this?
- How does this position us for success in the future?

## Drawbacks

- Why should we not do this?
- What kind on ongoing burden does this place on the team?

## Prior Art

- List prior art, the good and bad.
- Why can't we simply use or copy them?

## Alternatives

- What other approaches have been considered and why did you not choose them?
- How about not doing this at all?

## Outstanding Questions

- List any remaining questions.
- Use this to resolve ambiguity and collaborate with your team during the RFC process.
- *These must be resolved before the RFC can be merged.*

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Submit a PR with spike-level code _roughly_ demonstrating the change.
- [ ] Incremental change #1
- [ ] Incremental change #2
- [ ] ...

Note: This can be filled out during the review process.

## Future Improvements

- List any future improvements. Use this to keep your "plan of attack" scope small and project a sound design.
