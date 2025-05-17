# RFC 2025-04-20 - Vector Control Plane

Vector Control Plane - service which allows to effectively control vector instances

## Context

- [Vector Config Server](https://github.com/vectordotdev/vector/discussions/22709)

## Cross cutting concerns

- Separate repo for VCP sources
- GoLang as primarily language for implementation

## Scope

### In scope

* Initially implement service with two key features:
  * Multi Tap
  * Config Provider HTTP Source

### Out of scope

- TBD

## Pain

- Simplify vector configuration management
- Simplify searching events in low-level events producers inside high-scaled vector clusters

## Proposal

### User Experience

- TBD

### Implementation

- GoLang

## Rationale

- TBD

## Drawbacks

- Single point of failure for huge vector clusters - clients should understand what hey do


## Alternatives

- Not found

## Outstanding Questions

- TBD

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Separate repo in vector project
- [ ] Initial implementation
- [ ] Maintainers: ?? / ??
- [ ] Contibutors: ?? / ??

Note: This can be filled out during the review process.

## Future Improvements

- On-line Web Editor
