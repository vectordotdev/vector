# Contributing

First, thank you for contributing to Vector! The goal of this document is to
provide everything you need to start contributing to Vector. The
following TOC is sorted progressively, starting with the basics and
expanding into more specifics.

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [Assumptions](#assumptions)
1. [Your First Contribution](#your-first-contribution)
1. [Workflow](#workflow)
   1. [Git Branches](#git-branches)
   1. [Git Commits](#git-commits)
      1. [Style](#style)
      1. [Signing-off](#signing-off)
   1. [Github Pull Requests](#github-pull-requests)
      1. [Title](#title)
      1. [Single Concern](#single-concern)
      1. [Reviews & Approvals](#reviews--approvals)
      1. [Merge Style](#merge-style)
   1. [CI](#ci)
1. [Development](#development)
   1. [Setup](#setup)
   1. [The Basics](#the-basics)
      1. [Directory Structure](#directory-structure)
      1. [Makefile](#makefile)
      1. [Code Style](#code-style)
      1. [Feature flags](#feature-flags)
      1. [Documentation](#documentation)
      1. [Changelog](#changelog)
   1. [Dependencies](#dependencies)
   1. [Guidelines](#guidelines)
      1. [Sink Healthchecks](#sink-healthchecks)
   1. [Testing](#testing)
      1. [Sample Logs](#sample-logs)
      1. [Tips and Tricks](#tips-and-tricks)
   1. [Benchmarking](#benchmarking)
1. [Security](#security)
1. [Legal](#legal)
   1. [DCO](#dco)
      1. [Trivial changes](#trivial-changes)
   1. [Granted rights and copyright assignment](#granted-rights-and-copyright-assignment)
1. [FAQ](#faq)
   1. [Why a DCO instead of a CLA?](#why-a-dco-instead-of-a-cla)
   1. [If I’m contributing while an employee, do I still need my employer to sign something?](#if-i%E2%80%99m-contributing-while-an-employee-do-i-still-need-my-employer-to-sign-something)
   1. [What if I forgot to sign my commits?](#what-if-i-forgot-to-sign-my-commits)

<!-- /MarkdownTOC -->

## Assumptions

1. **You're familiar with [Github](https://github.com) and the pull request
   workflow.**
2. **You've read Vector's [docs](https://vector.dev/docs/).**
3. **You know about the [Vector community](https://vector.dev/community/).
   Please use this for help.**

## Your First Contribution

1. Ensure your change has an issue! Find an
   [existing issue][urls.existing_issues] or [open a new issue][urls.new_issue].
   * This is where you can get a feel if the change will be accepted or not.
     Changes that are questionable will have a `needs: approval` label.
2. One approved, [fork the Vector repository][urls.fork_repo] in your own
   Github account.
3. [Create a new Git branch][urls.create_branch].
4. Review the Vector [workflow](#workflow) and [development](#development).
5. Make your changes.
6. [Submit the branch as a pull request][urls.submit_pr] to the main Vector
   repo.

## Workflow

### Git Branches

_All_ changes must be made in a branch and submitted as [pull requests](#pull-requests).
Vector does not adopt any type of branch naming style, but please use something
descriptive of your changes.

### Git Commits

#### Style

Please ensure your commits are small and focused; they should tell a story of
your change. This helps reviewers to follow your changes, especially for more
complex changes.

#### Signing-off

Your commits must include a [DCO](https://developercertificate.org/) signature.
This is simpler than it sounds; it just means that all of your commits
must contain:

```text
Signed-off-by: Joe Smith <joe.smith@email.com>
```

Git makes this easy by adding the `-s` or `--signoff` flags when you commit:

```bash
git commit -sm 'My commit message'
```

We also included a `make signoff` target that handles this for you if
you forget.

### Github Pull Requests

Once your changes are ready you must submit your branch as a [pull \
request](https://github.com/timberio/vector/pulls).

#### Title

The pull request title must follow the format outlined in the [conventional \
commits spec](https://www.conventionalcommits.org).
[Conventional commits](https://www.conventionalcommits.org) is a standardized
format for commit messages. Vector only requires this format for commits on
the `master` branch. And because Vector squashes commits before merging
branches, this means that only the pull request title must conform to this
format. Vector performs a pull request check to verify the pull request title
in case you forget.

A list of allowed sub-categories is defined
[here](https://github.com/timberio/vector/tree/master/.github).

The following are all good examples of pull request titles:

```text
feat(new sink): new `xyz` sink
feat(tcp source): add foo bar baz feature
fix(tcp source): fix foo bar baz bug
chore: improve build process
docs: fix typos
```

#### Single Concern

We generally discourage large pull requests that are over 300-500 lines of diff.
This is usually a sign that the pull request is addressing multiple concerns.
If you would like to propose a larger change we suggest coming onto our
[chat channel](https://chat.vector.dev) and discuss it with one of our
engineers. This way we can talk through the solution and discuss if a change
that large is even needed! This overall will produce a quicker response to the
change and likely produce code that aligns better with our process.

#### Reviews & Approvals

All pull requests must be reviewed and approved by at least one Vector team
member. The review process is outlined in the [Review guide](REVIEWING.md).

#### Merge Style

All pull requests are squashed and merged. We generally discourage large pull
requests that are over 300-500 lines of diff. If you would like to propose
a change that is larger we suggest coming onto our gitter channel and
discuss it with one of our engineers. This way we can talk through the
solution and discuss if a change that large is even needed! This overall
will produce a quicker response to the change and likely produce code that
aligns better with our process.

### CI

Currently Vector uses [CircleCI](https://circleci.com). The build process
is defined in `/.circleci/config.yml`. This delegates heavily to the
[`distribution/docker`](/distribution/docker) folder where Docker images are
defined for all of our testing, building, verifying, and releasing.

Tests are run for all changes, and Circleci is responsible for releasing
updated versions of Vector through various channels.

## Development

### Setup

1. Install Rust via [`rustup`](https://rustup.rs/):

   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. [Install Docker](https://docs.docker.com/install/). Docker
   containers are used for mocking Vector's integrations.

3. [Install Ruby](https://www.ruby-lang.org/en/downloads/) and
   [Bundler 2](https://bundler.io/v2.0/guides/bundler_2_upgrade.html).
   They are used to build Vector's documentation.

### The Basics

#### Directory Structure

* [`/benches`](/benches) - Internal benchmarks.
* [`/config`](/config) - Public facing Vector config, included in releases.
* [`/distribution`](/distribution) - Distribution artifacts for various targets.
* [`/lib`](/lib) - External libraries that do not depend on `vector` but are used within the project.
* [`/proto`](/proto) - Protobuf definitions.
* [`/scripts`](/scripts) - Scripts used to generate docs and maintain the repo.
* [`/src`](/src) - Vector source.
* [`/tests`](/tests) - Various high-level test cases.
* [`/website`](/website) - Website and documentation files.

#### Makefile

Vector includes a [`Makefile`](/Makefile) in the root of the repo. This serves
as a high-level interface for common commands. Running `make` will produce
a list of make targets with descriptions. These targets will be referenced
throughout this document.

#### Code Style

We use `rustfmt` on `stable` to format our code and CI will verify that your
code follows
this format style. To run the following command make sure `rustfmt` has been
installed on the stable toolchain locally.

```bash
# To install rustfmt
rustup component add rustfmt

# To format the code
make fmt
```

#### Feature flags

When a new component (a source, transform, or sink) is added, it has to be put
behind a feature flag with the corresponding name. This ensures that it is
possible to customize Vector builds. See the `features` section in `Cargo.toml`
for examples.

In addition, during development of a particular component it is useful to
disable all other components to speed up compilation. For example, it is
possible to build and run tests only for `console` sink using

```bash
cargo test --lib --no-default-features --features sinks-console sinks::console
```

In case if the tests are already built and only the component file changed, it
is around 4 times faster than rebuilding tests with all features.

#### Documentation

Documentation is extremely important to the Vector project. Ideally, all
contributions that will change or add behavior to Vector should include the
relevant updates to the documentation website.

The project attempts to make documentation updates as easy as possible, reducing
most of it down to a few small changes which are outlined in
[DOCUMENTING.md](/DOCUMENTING.md).

Regardless of whether your changes require documentation updates you should
always run `make generate` before attempting to merge your commits.

#### Changelog

Developers do not need to maintain the [`Changelog`](/CHANGELOG.md). This is
automatically generated via the `make release` command. This is made possible
by the use of [conventional commit](#what-is-conventional-commits) titles.

### Dependencies

Dependencies should be _carefully_ selected and avoided if possible. You can
see how dependencies are reviewed in the
[Reviewing guide](/REVIEWING.md#dependencies).

If a dependency is required only by one or multiple components, but not by
Vector's core, make it optional and add it to the list of dependencies of
the features corresponding to these components in `Cargo.toml`.

### Guidelines

#### Sink Healthchecks

Sinks may implement a health check as a means for validating their configuration
against the environment and external systems. Ideally, this allows the system to
inform users of problems such as insufficient credentials, unreachable
endpoints, non-existent tables, etc. They're not perfect, however, since it's
impossible to exhaustively check for issues that may happen at runtime.

When implementing health checks, we prefer false positives to false negatives.
This means we would prefer that a health check pass and the sink then fail than
to have the health check fail when the sink would have been able to run
successfully.

A common cause of false negatives in health checks is performing an operation
that the sink itself does not need. For example, listing all of the available S3
buckets and checking that the configured bucket is on that list. The S3 sink
doesn't need the ability to list all buckets, and a user that knows that may not
have permitted it to do so. In that case, the health check will fail due
to bad credentials even through its credentials are sufficient for normal
operation.

This leads to a general strategy of mimicking what the sink itself does.
Unfortunately, the fact that health checks don't have real events available to
them leads to some limitations here. The most obvious example of this is with
sinks where the exact target of a write depends on the value of some field in
the event (e.g. an interpolated Kinesis stream name). It also pops up for sinks
where incoming events are expected to conform to a specific schema. In both
cases, random test data is reasonably likely to trigger a potentially
false-negative result. Even in simpler cases, we need to think about the effects
of writing test data and whether the user would find that surprising or
invasive. The answer usually depends on the system we're interfacing with.

In some cases, like the Kinesis example above, the right thing to do might be
nothing at all. If we require dynamic information to figure out what entity
(i.e. Kinesis stream in this case) that we're even dealing with, odds are very
low that we'll be able to come up with a way to meaningfully validate that it's
in working order. It's perfectly valid to have a health check that falls back to
doing nothing when there is a data dependency like this.

With all that in mind, here is a simple checklist to go over when writing a new
health check:

* [ ] Does this check perform different fallible operations from the sink itself?
* [ ] Does this check have side effects the user would consider undesirable (e.g. data pollution)?
* [ ] Are there situations where this check would fail but the sink would operate normally?

Not all of the answers need to be a hard "no", but we should think about the
likelihood that any "yes" would lead to false negatives and balance that against
the usefulness of the check as a whole for finding problems. Because we have the
option to disable individual health checks, there's an escape hatch for users
that fall into a false negative circumstance. Our goal should be to minimize the
likelihood of users needing to pull that lever while still making a good effort
to detect common problems.

### Testing

You can run Vector's tests via the `make test` command. Our tests use Docker
compose to spin up mock services for testing, such as
[localstack](https://github.com/localstack/localstack).

#### Sample Logs

We use `flog` to build a sample set of log files to test sending logs from a
file. This can be done with the following commands on mac with homebrew.
Installation instruction for flog can be found
[here](https://github.com/mingrammer/flog#installation).

```bash
flog --bytes $((100 * 1024 * 1024)) > sample.log
```

This will create a `100MiB` sample log file in the `sample.log` file.

#### Tips and Tricks

If you are developing a particular component and want to quickly iterate on unit
tests related only to this component, the following approach can reduce waiting
times:

1. Install [cargo-watch](https://github.com/passcod/cargo-watch).
2. (Only for GNU/Linux) Install LLVM 9 (for example, package `llvm-9` on Debian)
   and set `RUSTFLAGS` environment variable to use `lld` as the linker:

   ```sh
   export RUSTFLAGS='-Clinker=clang-9 -Clink-arg=-fuse-ld=lld'
   ```

3. Run in the root directory of Vector's source

   ```sh
   cargo watch -s clear -s \
     'cargo test --lib --no-default-features --features=<component type>-<component name> <component type>::<component name>'
   ```

   For example, if the component is `add_fields` transform, the command above
   turns into

   ```sh
   cargo watch -s clear -s \
     'cargo test --lib --no-default-features --features=transforms-add_fields transforms::add_fields'
   ```

### Benchmarking

All benchmarks are placed in the [`/benches`](/benches) folder. You can
run benchmarks via the `make benchmarks` command. In addition, Vector
maintains a full [test hardness][urls.vector_test_harness] for complex
end-to-end integration and performance testing.

## Security

Please see the [`SECURITY.md` file](/SECURITY.md).

## Legal

To protect all users of Vector, the following legal requirements are made.

### DCO

Vector requires all contributors to agree to the DCO. DCO stands for Developer
Certificate of Origin and is maintained by the
[Linux Foundation](https://www.linuxfoundation.org). It is an attestation
attached to every commit made by every developer. It ensures that all committed
code adheres to the [Vector license](LICENSE.md) (Apache 2.0).

#### Trivial changes

Trivial changes, such as spelling fixes, do not need to be signed.

### Granted rights and copyright assignment

It is important to note that the DCO is not a license. The license of the
project – in our case the Apache License – is the license under which the
contribution is made. However, the DCO in conjunction with the Apache License
may be considered an alternate CLA.

The existence of section 5 of the Apache License is proof that the Apache
License is intended to be usable without CLAs. Users need for the code to be
open-source, with all the legal rights that imply, but it is the open source
license that provides this. The Apache License provides very generous
copyright permissions from contributors, and contributors explicitly grant
patent licenses as well. These rights are granted to everyone.

## FAQ

### Why a DCO instead of a CLA?

It's simpler, clearer, and still protects users of Vector. We believe the DCO
more accurately embodies the principles of open-source. More info can be found
here:

* [Gitlab's switch to DCO](https://about.gitlab.com/2017/11/01/gitlab-switches-to-dco-license/)
* [DCO vs CLA](https://opensource.com/article/18/3/cla-vs-dco-whats-difference)

### If I’m contributing while an employee, do I still need my employer to sign something?

Nope! The DCO confirms that you are entitled to submit the code, which assumes
that you are authorized to do so.  It treats you like an adult and relies on
your accurate statement about your rights to submit a contribution.

### What if I forgot to sign my commits?

No probs! We made this simple with the [`signoff` Makefile target](Makefile):

```bash
make signoff
```

If you prefer to do this manually:

https://stackoverflow.com/questions/13043357/git-sign-off-previous-commits


[urls.create_branch]: https://help.github.com/en/github/collaborating-with-issues-and-pull-requests/creating-and-deleting-branches-within-your-repository
[urls.existing_issues]: https://github.com/timberio/vector/issues
[urls.fork_repo]: https://help.github.com/en/github/getting-started-with-github/fork-a-repo
[urls.github_sign_commits]: https://help.github.com/en/github/authenticating-to-github/signing-commits
[urls.new_issue]: https://github.com/timberio/vector/issues/new
[urls.submit_pr]: https://help.github.com/en/github/collaborating-with-issues-and-pull-requests/creating-a-pull-request-from-a-fork
[urls.vector_test_harness]: https://github.com/timberio/vector-test-harness/
