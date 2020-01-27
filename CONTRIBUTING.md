# Contributing

First, thank you for contributing to Vector! The goal of this document is to
clearly provide everything you need to start contributing to Vector. The
following TOC is sorted in a progressive fashion, starting with the basics and
expanding into more specifics.

<!-- MarkdownTOC autolink="true" indent="   " -->

- [Assumptions](#assumptions)
- [Workflow](#workflow)
   - [Git Branches](#git-branches)
   - [Git Commits](#git-commits)
      - [Style](#style)
      - [Signing](#signing)
   - [Github Pull Requests](#github-pull-requests)
      - [Title](#title)
      - [Merging](#merging)
   - [CI](#ci)
- [Development](#development)
   - [Setup](#setup)
   - [The Basics](#the-basics)
      - [Directory Structure](#directory-structure)
      - [Makefile](#makefile)
      - [Code Style](#code-style)
      - [Documentation](#documentation)
      - [Changelog](#changelog)
   - [Building a sink](#building-a-sink)
      - [Healthchecks](#healthchecks)
         - [Guidelines for writing healthchecks](#guidelines-for-writing-healthchecks)
   - [Testing](#testing)
      - [Sample Logs](#sample-logs)
   - [Benchmarking](#benchmarking)
- [FAQ](#faq)
   - [What is conventional commits?](#what-is-conventional-commits)
   - [Do I need to update the changelog?](#do-i-need-to-update-the-changelog)
   - [What is a DCO?](#what-is-a-dco)
   - [Why does Vector adopt the DCO?](#why-does-vector-adopt-the-dco)
   - [Why a DCO instead of a CLA?](#why-a-dco-instead-of-a-cla)
   - [What about trivial changes?](#what-about-trivial-changes)
   - [Granted rights and copyright assignment](#granted-rights-and-copyright-assignment)
   - [If I’m contributing while an employee, do I still need my employer to sign something?](#if-i%E2%80%99m-contributing-while-an-employee-do-i-still-need-my-employer-to-sign-something)
   - [What if I forgot to sign my commits?](#what-if-i-forgot-to-sign-my-commits)

<!-- /MarkdownTOC -->

## Assumptions

1. **You are familiar with the [docs](https://vector.dev/docs/).**
2. **You know about the [Vector community](https://vector.dev/community/),
   use this for help.**

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

#### Signing

Your commits must include a [DCO](https://developercertificate.org/) signature.
This is simpler than it sounds; it just means that all of your commits
must contain:

```
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
commits spec](https://www.conventionalcommits.org) (see the ["What is
conventional commits?" FAQ](#what-is-conventional-commits)). A list of allowed
sub categories is defined
[here](https://github.com/timberio/vector/tree/master/.github).

The follow are all good examples of pull request titles:

```
feat(new sink): new `xyz` sink
feat(tcp source): add foo bar baz feature
fix(tcp source): fix foo bar baz bug
chore: improve build process
docs: fix typos
```

#### Merging

At least one Vector team member must approve your work before merging. All
pull requests are squashed and merged. We generally discourage large pull
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

Tests are run for all changes, and Circleci is reponsible for releasing
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
* [`/docs`](/docs) - https://vector.dev/docs/ source.
* [`/lib`](/lib) - External libraries that do not depend on `vector` but are used within the project.
* [`/proto`](/proto) - Protobuf definitions.
* [`/scripts`](/scripts) - Scripts used to generate docs and maintain the repo.
* [`/tests`](/tests) - Various high-level test cases.

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
by the use of [conventional commit](#what-is-conventioonal-commits) titles.

### Building a sink

#### Healthchecks

Sinks may implement a healthcheck as a means for validating their configuration
against the envionment and external systems. Ideally, this allows the system to
inform users of problems such as insufficient credentials, unreachable
endpoints, non-existant tables, etc. They're not perfect, however, since it's
impossible to exhaustively check for issues that may happen at runtime.

##### Guidelines for writing healthchecks

When implementing healthchecks, we prefer false positives to false negatives.
This means we would prefer that a healthcheck pass and the sink then fail than
to have the healthcheck fail when the sink would have been able to run
successfully.

A common cause of false negatives in healthchecks is performing an operation
that the sink itself does not need. For example, listing all of the available S3
buckets and checking that the configured bucket is in that list. The S3 sink
doesn't need the ability to list all buckets, and a user that knows that may not
have given it permission to do so. In that case, the healthcheck will fail due
to bad credentials even through its credentials are sufficient for normal
operation.

This leads to a general strategy of mimicking what the sink itself does.
Unfortunately, the fact that healthchecks don't have real events available to
them leads to some limitations here. The most obvious example of this is with
sinks where the exact target of a write depends on the value of some field in
the event (e.g. an interpolated Kinesis stream name). It also pops up for sinks
where incoming events are expected to conform to a specific schema. In both
cases, random test data is reasonably likely to trigger a potentially false
negative result. Even in simpler cases, we need to think about the effects of
writing test data and whether the user would find that surprising or invasive.
The answer usually depends on the system we're interfacing with.

In some cases, like the Kinesis example above, the right thing to do might be
nothing at all. If we require dynamic information to figure out what entity
(i.e. Kinesis stream in this case) that we're even dealing with, odds are very
low that we'll be able to come up with a way to meaningfully validate that it's
in working order. It's perfectly valid to have a healthcheck that falls back to
doing nothing when there is a data dependency like this.

With all that in mind, here is a simple checklist to go over when writing a new
healthcheck:

- [ ] Does this check perform different fallible operations from the sink itself?
- [ ] Does this check have side effects the user would consider undesirable (e.g. data pollution)?
- [ ] Are there situations where this check would fail but the sink would operate normally?

Not all of the answers need to be a hard "no", but we should think about the
likelihood that any "yes" would lead to false negatives and balance that against
the usefulness of the check as a whole for finding problems. Because we have the
option to disable individual healthchecks, there's an escape hatch for users
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

### Benchmarking

All benchmarks are placed in the [`/benches`](/benches) folder. You can
run benchmarks via the `make benchmarks` command.

## FAQ

### What is conventional commits?

[Conventional commits](https://www.conventionalcommits.org) is a standardized
format for commit messages. Vector only requires this format for commits on
the `master` branch. And because Vector squashes commits before merging
branches, this means that only the pull request title must conform to this
format. Vector performs a pull request check to verify the pull request title
in case you forget.

### Do I need to update the changelog?

Nope! This is one of the primary reasons we use the conventional commits style.
Before releasing Vector we'll automatically generate a changelog for the
release.

### What is a DCO?

DCO stands for Developer Certificate of Origin and is maintained by the
[Linux Foundation](https://www.linuxfoundation.org). It is an attestation
attached to every commit made by every developer. It ensures that all committed
code adheres to the [Vector license](LICENSE.md) (Apache 2.0).

### Why does Vector adopt the DCO?

To protect the users of Vector. It ensures that all Vector contributors, and
committed code, agree to the [Vector license](LICENSE.md).

### Why a DCO instead of a CLA?

It's simpler, clearer, and still protects users of Vector. We believe the DCO
more accurately embodies the principles of open-source. More info can be found
here:

* [Gitlab's switch to DCO](https://about.gitlab.com/2017/11/01/gitlab-switches-to-dco-license/)
* [DCO vs CLA](https://opensource.com/article/18/3/cla-vs-dco-whats-difference)

### What about trivial changes?

Trivial changes, such as spelling fixes, do not need to be signed.

### Granted rights and copyright assignment

It is important to note that the DCO is not a license. The license of the
project – in our case the Apache License – is the license under which the
contribution is made. However, the DCO in conjunction with the Apache License
may be considered an alternate CLA.

The existence of section 5 of the Apache License is proof that the Apache
License is intended to be usable without CLAs. Users need for the code to be
open source, with all the legal rights that implies, but it is the open source
license that provides this. The Apache License provides very generous
copyright permissions from contributors, and contributors explicitly grant
patent licenses as well. These rights are granted to everyone.

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

