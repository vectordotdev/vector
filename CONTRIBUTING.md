# Contributing

First, thank you for contributing to Vector! The goal of this document is to
provide everything you need to start contributing to Vector. The
following TOC is sorted progressively, starting with the basics and
expanding into more specifics. Everyone from a first time contributor to a
Vector team member will find this document useful.

- [Introduction](#introduction)
- [Your First Contribution](#your-first-contribution)
  - [New sources, sinks, and transforms](#new-sources-sinks-and-transforms)
- [Workflow](#workflow)
  - [Git Branches](#git-branches)
  - [Git Commits](#git-commits)
    - [Style](#style)
  - [GitHub Pull Requests](#github-pull-requests)
    - [Title](#title)
    - [Reviews & Approvals](#reviews--approvals)
    - [Merge Style](#merge-style)
  - [CI](#ci)
    - [Releasing](#releasing)
    - [Testing](#testing)
      - [Skipping tests](#skipping-tests)
      - [Daily tests](#daily-tests)
    - [Flakey tests](#flakey-tests)
      - [Test harness](#test-harness)
  - [Running Tests Locally](#running-tests-locally)
  - [Deprecations](#deprecations)
  - [Dependencies](#dependencies)
- [Next steps](#next-steps)
- [Legal](#legal)
  - [Contributor License Agreement](#contributor-license-agreement)
  - [Granted rights and copyright assignment](#granted-rights-and-copyright-assignment)

## Introduction

1. **You're familiar with [GitHub](https://github.com) and the pull request
   workflow.**
2. **You've read Vector's [docs](https://vector.dev/docs/).**
3. **You know about the [Vector community](https://vector.dev/community/).
   Please use this for help.**

## Your First Contribution

1. Ensure your change has an issue! Find an
   [existing issue][urls.existing_issues] or [open a new issue][urls.new_issue].
   - This is where you can get a feel if the change will be accepted or not.
     Changes that are questionable will have a `needs: approval` label.
2. Once approved, [fork the Vector repository][urls.fork_repo] in your own
   GitHub account (only applicable to outside contributors).
3. [Create a new Git branch][urls.create_branch].
4. Make your changes.
5. [Submit the branch as a pull request][urls.submit_pr] to the main Vector
   repo. A Vector team member should comment and/or review your pull request
   within a few days. Although, depending on the circumstances, it may take
   longer.

### New sources, sinks, and transforms

If you're thinking of contributing a new source, sink, or transform to Vector, thank you that's way cool! The answers to
the below questions are required for each newly proposed component and depending on the answers, we may elect to not
include the proposed component. If you're having trouble with any of the questions, we're available to help you.

**Prior to beginning work on a new source or sink if a GitHub Issue does not already exist, please open one to discuss
the introduction of the new integration.** Maintainers will review the proposal with the following checklist in mind,
try and consider them when sharing your proposal to reduce the amount of time it takes to review your proposal. This
list is not exhaustive, and may be updated over time.

- [ ] Can the proposed component’s functionality be replicated by an existing component, with a specific configuration?
(ex: Azure Event Hub as a `kafka` sink configuration)
  - [ ] Alternatively implemented as a wrapper around an existing component. (ex. `axiom` wrapping `elasticsearch`)
- [ ] Can an existing component replicate the proposed component’s functionality, with non-breaking changes?
- [ ] Can an existing component be rewritten in a more generic fashion to cover both the existing and proposed functions?
- [ ] Is the proposed component generically usable or is it specific to a particular service?
  - [ ] How established is the target of the integration, what is the relative market share of the integrated service?
- [ ] Is there sufficient demand for the component?
  - [ ] If the integration can be served with a workaround or more generic component, how painful is this for users?
- [ ] Is the contribution from an individual or the organization owning the integrated service? (examples of
organization backed integrations: `databend` sink, `axiom` sink)
  - [ ] Is the contributor committed to maintaining the integration if it is accepted?
- [ ] What is the overall complexity of the proposed design of this integration from a technical and functional
standpoint, and what is the expected ongoing maintenance burden?
- [ ] How will this integration be tested and QA’d for any changes and fixes?
  - [ ] Will we have access to an account with the service if the integration is not open source?

To merge a new source, sink, or transform, the pull request is required to:

- [ ] Add tests, especially integration tests if your contribution connects to an external service.
- [ ] Add instrumentation so folks using your integration can get insight into how it's working and performing. You can
see some [example of instrumentation in existing integrations](https://github.com/vectordotdev/vector/tree/master/src/internal_events).
- [ ] Add documentation. You can see [examples in the `docs` directory](https://github.com/vectordotdev/vector/blob/master/docs).

When adding new integration tests, the following changes are needed in the GitHub Workflows:

- in `.github/workflows/integration.yml`, add another entry in the matrix definition for the new integration.
- in `.github/workflows/integration-comment.yml`, add another entry in the matrix definition for the new integration.
- in `.github/workflows/changes.yml`, add a new filter definition for files changed, and update the `changes` job
outputs to reference the filter, and finally update the outputs of `workflow_call` to include the new filter.

## Workflow

### Git Branches

_All_ changes must be made in a branch and submitted as [pull requests](#github-pull-requests).

If you want your branch to have a website preview build created, include the word `website` in the
branch.

Otherwise, Vector does not adopt any type of branch naming style, but please use something
descriptive of your changes.

### Git Commits

#### Style

Please ensure your commits are small and focused; they should tell a story of
your change. This helps reviewers to follow your changes, especially for more
complex changes.

### GitHub Pull Requests

Once your changes are ready you must submit your branch as a [pull request](https://github.com/vectordotdev/vector/pulls).

#### Title

The pull request title must follow the format outlined in the [conventional commits spec](https://www.conventionalcommits.org).
[Conventional commits](https://www.conventionalcommits.org) is a standardized
format for commit messages. Vector only requires this format for commits on
the `master` branch. And because Vector squashes commits before merging
branches, this means that only the pull request title must conform to this
format. Vector performs a pull request check to verify the pull request title
in case you forget.

A list of allowed sub-categories is defined
[here](https://github.com/vectordotdev/vector/blob/master/.github/semantic.yml#L21).

The following are all good examples of pull request titles:

```text
feat(new sink): new `xyz` sink
feat(tcp source): add foo bar baz feature
fix(tcp source): fix foo bar baz bug
chore: improve build process
docs: fix typos
```

#### Reviews & Approvals

All pull requests should be reviewed by:

- No review required for cosmetic changes like whitespace, typos, and spelling
  by a maintainer
- One Vector team member for minor changes or trivial changes from contributors
- Two Vector team members for major changes
- Three Vector team members for RFCs

If CODEOWNERS are assigned, a review from an individual from each of the sets of owners is required.

#### Merge Style

All pull requests are squashed and merged. We generally discourage large pull
requests that are over 300-500 lines of diff. If you would like to propose a
change that is larger we suggest coming onto our [Discord server](https://chat.vector.dev/) and discuss it
with one of our engineers. This way we can talk through the solution and
discuss if a change that large is even needed! This will produce a quicker
response to the change and likely produce code that aligns better with our
process.

#### Changelog

By default, all pull requests are assumed to include user-facing changes that
need to be communicated in the project's changelog. If your pull request does
not contain user-facing changes that warrant describing in the changelog, add
the label 'no-changelog' to your PR. When in doubt, consult the vector team
for guidance. The details on how to add a changelog entry for your PR are
outlined in detail in [changelog.d/README.md](changelog.d/README.md).

### CI

Currently, Vector uses GitHub Actions to run tests. The workflows are defined in
`.github/workflows`.

#### Releasing

GitHub Actions is responsible for releasing updated versions of Vector through
various channels.

#### Testing

##### Skipping tests

Tests are run for all changes except those that have the label:

```text
ci-condition: skip
```

##### Daily tests

Some long-running tests are only run daily, rather than on every pull request.
If needed, an administrator can kick off these tests manually via the button on
the [nightly build action
page](https://github.com/vectordotdev/vector/actions?query=workflow%3Anightly)

#### Flakey tests

Historically, we've had some trouble with tests being flakey. If your PR does
not have passing tests:

- Ensure that the test failures are unrelated to your change
  - Is it failing on master?
  - Does it fail if you rerun CI?
  - Can you reproduce locally?
- Find or open an issue for the test failure
  ([example](https://github.com/vectordotdev/vector/issues/3781))
- Link the PR in the issue for the failing test so that there are more examples

##### Test harness

You can invoke the [test harness][urls.vector_test_harness] by commenting on
any pull request with:

```bash
/test -t <name>
```

### Running Tests Locally

To run tests locally, use [cargo vdev](https://github.com/vectordotdev/vector/blob/master/vdev/README.md).

Unit tests can be run by calling `cargo vdev test`.

Integration tests are not run by default when running
`cargo vdev test`. Instead, they are accessible via the integration subcommand (example:
`cargo vdev int test aws` runs aws-related integration tests). You can find the list of available integration tests using `cargo vdev int show`. Integration tests require docker or podman to run.

### Running other checks

There are other checks that are run by CI before the PR can be merged. These should be run locally
first to ensure they pass.

```sh
# Run the Clippy linter to catch common mistakes.
cargo vdev check rust --clippy
# Ensure all code is properly formatted. Code can be run through `rustfmt` using `cargo fmt` to ensure it is properly formatted.
cargo vdev check fmt
# Ensure the internal metrics that Vector emits conform to standards.
cargo vdev check events
# Ensure the `LICENSE-3rdparty.csv` file is up to date with the licenses each of Vector's dependencies are published under.
cargo vdev check licenses
# Vector's documentation for each component is generated from the comments attached to the Component structs and members.
# Running this ensures that the generated docs are up to date.
make check-component-docs
# Generate the code documentation for the Vector project.
# Run this to ensure the docs can be generated without errors (warnings are acceptable at the minute).
cd rust-doc && make docs
```

### Deprecations

When deprecating functionality in Vector, see [DEPRECATION.md](docs/DEPRECATION.md).

### Dependencies

When adding, modifying, or removing a dependency in Vector you may find that you need to update the
inventory of third-party licenses maintained in `LICENSE-3rdparty.csv`. This file is generated using
[dd-rust-license-tool](https://github.com/DataDog/rust-license-tool.git) and can be updated using
`cargo vdev build licenses`.

## Next steps

As discussed in the [`README`](README.md), you should continue to the following
documents:

1. **[DEVELOPING.md](docs/DEVELOPING.md)** - Everything necessary to develop
2. **[DOCUMENTING.md](docs/DOCUMENTING.md)** - Preparing your change for Vector users
3. **[DEPRECATION.md](docs/DEPRECATION.md)** - Deprecating functionality in Vector

## Legal

To protect all users of Vector, the following legal requirements are made.
If you have additional questions, please [contact us].

### Contributor License Agreement

Vector requires all contributors to sign the Contributor License Agreement
(CLA). This gives Vector the right to use your contribution as well as ensuring
that you own your contributions and can use them for other purposes.

The full text of the CLA can be found at [https://cla.datadoghq.com/vectordotdev/vector](https://cla.datadoghq.com/vectordotdev/vector).

### Granted rights and copyright assignment

This is covered by the CLA.

[contact us]: https://vector.dev/community
[urls.create_branch]: https://help.github.com/en/github/collaborating-with-issues-and-pull-requests/creating-and-deleting-branches-within-your-repository
[urls.existing_issues]: https://github.com/vectordotdev/vector/issues
[urls.fork_repo]: https://help.github.com/en/github/getting-started-with-github/fork-a-repo
[urls.new_issue]: https://github.com/vectordotdev/vector/issues/new
[urls.submit_pr]: https://help.github.com/en/github/collaborating-with-issues-and-pull-requests/creating-a-pull-request-from-a-fork
[urls.vector_test_harness]: https://github.com/vectordotdev/vector-test-harness/
