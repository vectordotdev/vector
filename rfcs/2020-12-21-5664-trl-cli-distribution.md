# RFC 5664 - 2020-12-21 - Distribution of the `trl` CLI tool

The `trl` CLI tool for the Timber Remap Language is an extremely useful accompaniment to the
language, but at the moment you can only use it inside of the Vector repo using `cargo run`. This
RFC proposes distributing the `trl` tool via the same channels that we currently distribute Vector
itself.

## Scope

This RFC only covers distribution of the `trl` tool as well as some related issues. It does *not*
cover TRL syntax, functions, semantics, and the like.

## Motivation

TRL is likely to be widely used and a crucial part of Vector. Although TRL is relatively easy to use
in conjunction with real Vector instances and topologies, e.g. via unit tests, the `trl` CLI tool
allows for much speedier trial and error and thus will be an essential part of development flows
for Vector.

Asking users to run the CLI tool by cloning the Vector repo, installing Rust/Cargo, and running
the command manually could be a significant impediment to TRL adoption.

## Internal Proposal

### General build setup

Our build setup—`Makefile`, scripts, etc.—would need to be revised pretty significantly, as what we
currently have revolves around a single artifact: Vector itself. We would need separate scripts and
`make` commands for `trl` and to update our GitHub Actions CI setup, most notably
[`release.yml`][release_yml].

### Release and versioning

The `trl` tool should be versioned in lockstep with and released alongside Vector, in order to
ensure full parity between the syntax, functions, and semantics available in `trl` and in the Vector
compilation and interpretation engine.

## Doc-level Proposal

On the documentation side, we would need to provide installation docs analogous to those we have
[for Vector itself][vector_install], though potentially collapsed into a single page for ease of
lookup. In addition, the [TRL docs][trl_docs] should briefly describe the tool and link to the
installation docs.

## Rationale

Enabling users to easily install and run the `trl` tool could be a major driver of TRL adoption and
of immediate benefit to Vector users. Imagine this flow on macOS:

```bash
brew tap timberio/brew
brew install trl
trl --example # This command doesn't exist but we could potentially provide something like this
```

Within just a few commands, a user can go from having nothing Vector related on their machine to
experimenting with the language. People who are already familiar with Vector will be able to quickly
see it can be integrated into their own topologies, and people who aren't familiar with Vector will
have their interest in the project piqued.

## Prior Art

The packaging and distribution of Vector itself can be seen as prior art. In some places we may be
able to more or less copy and paste `cargo build` and other logic, though we shouldn't over-assume
that this will always be the case.

## Drawbacks

Another artifact to package, release, distribute, document, maintain, etc. does introduce continuing
humanpower burdens. This RFC presumes that those burdens are worthwhile but should be rejected if
it's deemed otherwise.

## Alternatives

The alternative would be to continue requiring would-be users to clone the repo, install Rust, and
run `trl` manually. It's not clear that there's a desirable third alternative between the status quo
and full distribution.

## Plan Of Attack

- [ ] Update the `remap-cli` crate's version to match Vector itself (it's currently locked into `0.1.0`)
- [ ] Update build scripts to accommodate the new `trl` artifact
- [ ] Update `Makefile` to include handy commands for building locally
- [ ] Update GitHub Actions workflows
- [ ] Update documentation to include installation instructions
- [ ] Write an announcement blog post for the initial `trl` release

[release_yml]: https://github.com/timberio/vector/blob/master/.github/workflows/release.yml
[trl_docs]: https://vector.dev/docs/reference/remap
[vector_install]: https://vector.dev/docs/setup/installation
