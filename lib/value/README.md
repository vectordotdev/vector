# Value

This library contains the following types, shared across Vector libraries:

- `Kind` — The main type to support progressive type-checking in both Vector and
  VRL.

- ~~`Value`~~ — _soon_, an experiment to share a common `Value` type across
  Vector and VRL, reducing the need for allocations when moving data between the
  two.
