# Contributing

First, thank you for contributing to Vector! The goal of this document is to
provide everything you need to start contributing to Vector. The
following TOC is sorted progressively, starting with the basics and
expanding into more specifics.

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [Introduction](#introduction)
1. [Your First Contribution](#your-first-contribution)
   1. [New sources, sinks, and transforms](#new-sources-sinks-and-transforms)
1. [Change Control](#change-control)
   1. [Git Branches](#git-branches)
   1. [Git Commits](#git-commits)
      1. [Style](#style)
      1. [Signing-off](#signing-off)
   1. [Github Pull Requests](#github-pull-requests)
      1. [Title](#title)
      1. [Reviews & Approvals](#reviews--approvals)
      1. [Bors review process](#bors-review-process)
      1. [Merge Style](#merge-style)
   1. [CI](#ci)
      1. [Releasing](#releasing)
      1. [Testing](#testing)
         1. [Skipping tests](#skipping-tests)
         1. [Daily tests](#daily-tests)
      1. [Flakey tests](#flakey-tests)
         1. [Test harness](#test-harness)
1. [Development](#development)
   1. [Setup](#setup)
      1. [Using a Docker or Podman environment](#using-a-docker-or-podman-environment)
      1. [Bring your own toolbox](#bring-your-own-toolbox)
   1. [The Basics](#the-basics)
      1. [Directory Structure](#directory-structure)
      1. [Makefile](#makefile)
      1. [Code Style](#code-style)
         1. [Logging style](#logging-style)
      1. [Feature flags](#feature-flags)
      1. [Dependencies](#dependencies)
   1. [Guidelines](#guidelines)
      1. [Sink Healthchecks](#sink-healthchecks)
      1. [Metric naming convention](#metric-naming-convention)
      1. [Option naming](#option-naming)
   1. [Testing](#testing-1)
      1. [Unit Tests](#unit-tests)
      1. [Integration Tests](#integration-tests)
      1. [Blackbox Tests](#blackbox-tests)
      1. [Tips and Tricks](#tips-and-tricks)
         1. [Testing Specific Components](#testing-specific-components)
         1. [Generating Sample Logs](#generating-sample-logs)
   1. [Benchmarking](#benchmarking)
   1. [Profiling](#profiling)
   1. [Kubernetes](#kubernetes)
      1. [Kubernetes Dev Flow](#kubernetes-dev-flow)
         1. [Requirements](#requirements)
         1. [The dev flow](#the-dev-flow)
         1. [Troubleshooting](#troubleshooting)
         1. [Going through the dev flow manually](#going-through-the-dev-flow-manually)
      1. [Kubernetes E2E tests](#kubernetes-e2e-tests)
         1. [Requirements](#requirements-1)
         1. [Running the E2E tests](#running-the-e2e-tests)
1. [Humans](#humans)
   1. [Documentation](#documentation)
   1. [Changelog](#changelog)
      1. [What makes a highlight noteworthy?](#what-makes-a-highlight-noteworthy)
      1. [How is a highlight different from a blog post?](#how-is-a-highlight-different-from-a-blog-post)
1. [Security](#security)
1. [Legal](#legal)
   1. [DCO](#dco)
      1. [Trivial changes](#trivial-changes)
   1. [Granted rights and copyright assignment](#granted-rights-and-copyright-assignment)
1. [FAQ](#faq)
   1. [Why a DCO instead of a CLA?](#why-a-dco-instead-of-a-cla)
   1. [If I’m contributing while an employee, do I still need my employer to sign something?](#if-i%E2%80%99m-contributing-while-an-employee-do-i-still-need-my-employer-to-sign-something)
   1. [What if I forgot to sign my commits?](#what-if-i-forgot-to-sign-my-commits)
1. [Contact](#contact)

<!-- /MarkdownTOC -->

## Introduction

1. **You're familiar with [Github](https://github.com) and the pull request
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
   Github account.
3. [Create a new Git branch][urls.create_branch].
4. Review the Vector [change control](#change-control) and [development](#development) workflows.
5. Make your changes.
6. [Submit the branch as a pull request][urls.submit_pr] to the main Vector
   repo. A Vector team member should comment and/or review your pull request
   within a few days. Although, depending on the circumstances, it may take
   longer.

### New sources, sinks, and transforms

If you're contributing a new source, sink, or transform to Vector, thank you that's way cool! There's a few steps you need think about if you want to make sure we can merge your contribution. We're here to help you along with these steps but they are a blocker to getting a new integration released.

To merge a new source, sink, or transform, you need to:

- [ ] Add tests, especially integration tests if your contribution connects to an external service.
- [ ] Add instrumentation so folks using your integration can get insight into how it's working and performing. You can see some [example of instrumentation in existing integrations](https://github.com/timberio/vector/tree/master/src/internal_events).
- [ ] Add documentation. You can see [examples in the `docs` directory](https://github.com/timberio/vector/blob/master/docs).
- [ ] Update [`.github/CODEOWNERS`](https://github.com/timberio/vector/blob/master/.github/CODEOWNERS) or talk to us about identifying someone on the team to help look after the new integration.

## Change Control

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

#### Reviews & Approvals

All pull requests should be reviewed by:

- No review required for cosmetic changes like whitespace, typos, and spelling
  by a maintainer
- One Vector team member for minor changes or trivial changes from contributors
- Two Vector team members for major changes
- Three Vector team members for RFCs

If there are any CODEOWNERs automatically assigned, you should also wait for
their review.

#### Bors review process

[![Bors enabled](https://bors.tech/images/badge_small.svg)](https://app.bors.tech/repositories/28346)

Once you’ve reviewed the PR, instead of clicking the green “Merge Button”, leave a comment like this on the pull request:

```text
bors r+
```

Equivalently, you can comment the following:

```text
bors merge
```

The pull request, as well as any other pull requests that are reviewed around the same time, will be merged into a branch called `staging`. CI will run there and report the result back. If that result is “OK”, `master` gets fast-forwarded to reach it.

There’s also:

```text
bors try
```

When this is run, your branch and master get merged into `trying`, and bors will report the results just like the `staging` branch would. Only reviewers can push to this.

The review process is outlined in the [Review guide](REVIEWING.md).

#### Merge Style

All pull requests are squashed and merged. We generally discourage large pull
requests that are over 300-500 lines of diff. If you would like to propose a
change that is larger we suggest coming onto our [Discord server](https://chat.vector.dev/) and discuss it
with one of our engineers. This way we can talk through the solution and
discuss if a change that large is even needed! This will produce a quicker
response to the change and likely produce code that aligns better with our
process.

### CI

Currently Vector uses Github Actions to run tests. The workflows are defined in
`.github/workflows`.

#### Releasing

Github Actions is responsible for releasing updated versions of Vector through
various channels.

#### Testing

##### Skipping tests

Tests are run for all changes except those that have the label:

```text
ci-condition: skip
```

##### Daily tests

Some long running tests are only run daily, rather than on every pull request.
If needed, an administrator can kick off these tests manually via the button on
the [nightly build action
page](https://github.com/timberio/vector/actions?query=workflow%3Anightly)

#### Flakey tests

Historically, we've had some trouble with tests being flakey. If your PR does
not have passing tests:

- Ensure that the test failures are unrelated to your change
  - Is it failing on master?
  - Does it fail if you rerun CI?
  - Can you reproduce locally?
- Find or open an issue for the test failure
  ([example](https://github.com/timberio/vector/issues/3781))
- Link the PR in the issue for the failing test so that there are more examples

##### Test harness

You can invoke the [test harness][urls.vector_test_harness] by commenting on
any pull request with:

```bash
/test -t <name>
```

## Development

### Setup

We're super excited to have you interested in working on Vector! Before you start you should pick how you want to develop.

For small or first-time contributions, we recommend the Docker method. Prefer to do it yourself? That's fine too!

#### Using a Docker or Podman environment

> **Targets:** You can use this method to produce AARCH64, Arm6/7, as well as x86/64 Linux builds.

Since not everyone has a full working native environment, we took our environment and stuffed it into a Docker (or Podman) container!

This is ideal for users who want it to "Just work" and just want to start contributing. It's also what we use for our CI, so you know if it breaks we can't do anything else until we fix it. 😉

**Before you go farther, install Docker or Podman through your official package manager, or from the [Docker](https://docs.docker.com/get-docker/) or [Podman](https://podman.io/) sites.**

```bash
# Optional: Only if you use `podman`
export CONTAINER_TOOL="podman"
```

By default, `make environment` style tasks will do a `docker pull` from Github's container repository, you can **optionally** build your own environment while you make your morning coffee ☕:

```bash
# Optional: Only if you want to go make a coffee
make environment-prepare
```

Now that you have your coffee, you can enter the shell!

```bash
# Enter a shell with optimized mounts for interactive processes.
# Inside here, you can use Vector like you have full toolchain (See below!)
make environment
# Try out a specific container tool. (Docker/Podman)
make environment CONTAINER_TOOL="podman"
# Add extra cli opts
make environment CLI_OPTS="--publish 3000:2000"
```

Now you can use the jobs detailed in **"Bring your own toolbox"** below.

Want to run from outside of the environment? _Clever. Good thinking._ You can run any of the following:

```bash
# Validate your code can compile
make check ENVIRONMENT=true
# Validate your code actually does compile (in dev mode)
make build-dev ENVIRONMENT=true
# Validate your test pass
make test SCOPE="sources::example" ENVIRONMENT=true
# Validate tests (that do not require other services) pass
make test ENVIRONMENT=true
# Validate your tests pass (starting required services in Docker)
make test-integration SCOPE="sources::example" ENVIRONMENT=true
# Validate your tests pass against a live service.
make test-integration SCOPE="sources::example" AUTOSPAWN=false ENVIRONMENT=true
# Validate all tests pass (starting required services in Docker)
make test-integration ENVIRONMENT=true
# Run your benchmarks
make bench SCOPE="transforms::example" ENVIRONMENT=true
# Format your code before pushing!
make fmt ENVIRONMENT=true
```

We use explicit environment opt-in as many contributors choose to keep their Rust toolchain local.

#### Bring your own toolbox

> **Targets:** This option is required for MSVC/Mac/FreeBSD toolchains. It can be used to build for any environment or OS.

To build Vector on your own host will require a fairly complete development environment!

We keep an up to date list of all dependencies used in our CI environment inside our `default.nix` file. Loosely, you'll need the following:

- **To build Vector:** Have working Rustup, Protobuf tools, C++/C build tools (LLVM, GCC, or MSVC), Python, and Perl, `make` (the GNU one preferably), `bash`, `cmake`, and `autotools`. (Full list in [`scripts/environment/definition.nix`](./scripts/environment/definition.nix).
- **To run integration tests:** Have `docker` available, or a real live version of that service. (Use `AUTOSPAWN=false`)
- **To run `make check-component-features`:** Have `remarshal` installed.

If you find yourself needing to run something inside the Docker environment described above, that's totally fine, they won't collide or hurt each other. In this case, you'd just run `make environment-generate`.

We're interested in reducing our dependencies if simple options exist. Got an idea? Try it out, we'd to hear of your successes and failures!

In order to do your development on Vector, you'll primarily use a few commands, such as `cargo` and `make` tasks you can use ordered from most to least frequently run:

```bash
# Validate your code can compile
cargo check
make check
# Validate your code actually does compile (in dev mode)
cargo build
make build-dev
# Validate your test pass
cargo test sources::example
make test scope="sources::example"
# Validate tests (that do not require other services) pass
cargo test
make test
# Validate your tests pass (starting required services in Docker)
make test-integration scope="sources::example" autospawn=false
# Validate your tests pass against a live service.
make test-integration scope="sources::example" autospawn=false
cargo test --features docker sources::example
# Validate all tests pass (starting required services in Docker)
make test-integration
# Run your benchmarks
make bench scope="transforms::example"
cargo bench transforms::example
# Format your code before pushing!
make fmt
cargo fmt
```

If you run `make` you'll see a full list of all our tasks. Some of these will start Docker containers, sign commits, or even make releases. These are not common development commands and your mileage may vary.

### The Basics

#### Directory Structure

- [`/.meta`](/.meta) - Project metadata used to generate documentation.
- [`/benches`](/benches) - Internal benchmarks.
- [`/config`](/config) - Public facing Vector config, included in releases.
- [`/distribution`](/distribution) - Distribution artifacts for various targets.
- [`/lib`](/lib) - External libraries that do not depend on `vector` but are used within the project.
- [`/proto`](/proto) - Protobuf definitions.
- [`/scripts`](/scripts) - Scripts used to generate docs and maintain the repo.
- [`/src`](/src) - Vector source.
- [`/tests`](/tests) - Various high-level test cases.

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

##### Logging style


- Always use the [Tracing crate](https://tracing.rs/tracing/)'s key/value style for log events.
- Events should be capitalized and end with a period, `.`.
- Never use `e` or `err` - always spell out `error` to enrich logs and make it
  clear what the output is.
- Prefer Display over Debug, `%error` and not `?error`.

Nope!

```rust
warn!("Failed to merge value: {}.", err);
```

Yep!

```rust
warn!(message = "Failed to merge value.", %error);
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

#### Dependencies

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

- [ ] Does this check perform different fallible operations from the sink itself?
- [ ] Does this check have side effects the user would consider undesirable (e.g. data pollution)?
- [ ] Are there situations where this check would fail but the sink would operate normally?

Not all of the answers need to be a hard "no", but we should think about the
likelihood that any "yes" would lead to false negatives and balance that against
the usefulness of the check as a whole for finding problems. Because we have the
option to disable individual health checks, there's an escape hatch for users
that fall into a false negative circumstance. Our goal should be to minimize the
likelihood of users needing to pull that lever while still making a good effort
to detect common problems.

#### Metric naming convention

For metrics naming, Vector broadly follows the [Prometheus metric naming standards](https://prometheus.io/docs/practices/naming/). Hence, a metric name:

- Must only contain valid characters, which are ASCII letters and digits, as well as underscores. It should match the regular expression: `[a-z_][a-z0-9_]*`.
- Metrics have a broad template:

  `<namespace>_<name>_<unit>_[total]`

  - The `namespace` is a single word prefix that groups metrics from a specific source, for example host-based metrics like CPU, disk, and memory are prefixed with `host`, Apache metrics are prefixed with `apache`, etc.
  - The `name` describes what the metric measures.
  - The `unit` is a [single base unit](https://en.wikipedia.org/wiki/SI_base_unit), for example seconds, bytes, metrics.
  - The suffix should describe the unit in plural form: seconds, bytes. Accumulating counts, both with units or without, should end in `total`, for example `disk_written_bytes_total` and `http_requests_total`.

- Where required, use tags to differentiate the characteristic of the measurement. For example, whilst `host_cpu_seconds_total` is name of the metric, we also record the `mode` that is being used for each CPU. The `mode` and the specific CPU then become tags on the metric:

```text
host_cpu_seconds_total{cpu="0",mode="idle"}
host_cpu_seconds_total{cpu="0",mode="idle"}
host_cpu_seconds_total{cpu="0",mode="nice"}
host_cpu_seconds_total{cpu="0",mode="system"}
host_cpu_seconds_total{cpu="0",mode="user"}
host_cpu_seconds_total
```

#### Option naming

When naming options for sinks, sources, and transforms it's important to keep in mind these guidelines:

- Suffix options with their unit. Ex: `_seconds`, `_bytes`, etc.
- Don't repeat the name space in the option name, ex. `fingerprinting.fingerprint_bytes`.
- Normalize around time units where relevant and possible, for example using seconds consistently rather than seconds and milliseconds.
- Use nouns as category names, for example `fingerprint` instead of `fingerprinting`.

### Testing

Testing is very important since Vector's primary design principle is reliability.
You can read more about how Vector tests in our
[testing blog post](https://vector.dev/blog/how-we-test-vector/).

#### Unit Tests

Unit tests refer to the majority of inline tests throughout Vector's code. A
defining characteristic of unit tests is that they do not require external
services to run, therfore they should be much quicker. You can run them with:

```bash
cargo test
```

#### Integration Tests

Integration tests verify that Vector actually works with the services it
integrates with. Unlike unit tests, integration tests require external services
to run. A few rules when setting up integration tests:

- [ ] To ensure all contributors can run integration tests, the service must
      run in a Docker container.
- [ ] The service must be configured on a unique port that is configured through
      an environment variable.
- [ ] Add a `test-integration-<name>` to Vector's [`Makefile`](/Makefile) and
      ensure that it starts the service before running the integration test.
- [ ] Add a `test-integration-<name>` job to Vector's
      [`.github/workflows/test.yml`](.github/workflows/test.yml) workflow and
      call your make target accordingly.

Once complete, you can run your integration tests with:

```bash
make test-integration-<name>
```

#### Blackbox Tests

Vector also offers blackbox testing via
[Vector's test harness][urls.vector_test_harness]. This is a complex testing
suite that tests Vector's performance in real-world environments. It is
typically used for benchmarking, but also correctness testing.

You can run these tests within a PR as described in the [CI section](#ci).

#### Tips and Tricks

##### Testing Specific Components

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

##### Generating Sample Logs

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
run benchmarks via the `make bench` command. In addition, Vector
maintains a full [test harness][urls.vector_test_harness] for complex
end-to-end integration and performance testing.

### Profiling

If you're trying to improve Vector's performance (or understand why your change
made it worse), profiling is a useful tool for seeing where time is being spent.

While there are a bunch of useful profiling tools, a simple place to get started
is with Linux's `perf`. Before getting started, you'll likely need to give
yourself access to collect stats:

```sh
echo -1 | sudo tee /proc/sys/kernel/perf_event_paranoid
```

You'll also want to edit `Cargo.toml` and make sure that Vector is being built
with debug symbols in release mode. This ensures that you'll get human-readable
info in the eventual output:

```toml
[profile.release]
debug = true
```

Then you can start up a release build of Vector with whatever config you're
interested in profiling.

```sh
cargo run --release -- --config my_test_config.toml
```

Once it's started, use the `ps` tool (or equivalent) to make a note of its PID.
We'll use this to tell `perf` which process we would like it to collect data
about.

The next step is somewhat dependent on the config you're testing. For this
example, let's assume you're using a simple TCP-mode socket source listening on
port 9000. Let's also assume that you have a large file of example input in
`access.log` (you can use a tool like `flog` to generate this).

With all that prepared, we can send our test input to Vector and collect data
while it is under load:

```sh
perf record -F99 --call-graph dwarf -p $VECTOR_PID socat -dd OPEN:access.log TCP:localhost:9000
```

This instructs `perf` to collect data from our already-running Vector process
for the duration of the `socat` command. The `-F` argument is the frequency at
which `perf` should sample the Vector call stack. Higher frequencies will
collect more data and produce more detailed output, but can produce enormous
amounts of data that take a very long time to process. Using `-F99` works well
when your input data is large enough to take a minute or more to process, but
feel free to adjust both input size and sampling frequency for your setup.

It's worth noting that this is not the normal way to profile programs with
`perf`. Usually you would simply run something like `perf record my_program` and
not have to worry about PIDs and such. We differ from this because we're only
interested in data about what Vector is doing while under load. Running it
directly under `perf` would collect data for the entire lifetime of the process,
including startup, shutdown, and idle time. By telling `perf` to collect data
only while the load generation command is running we get a more focused dataset
and don't have to worry about timing different commands in quick succession.

You'll now find a `perf.data` file in your current directory with all of the
information that was collected. There are different ways to process this, but
one of the most useful is to create
a [flamegraph](http://www.brendangregg.com/flamegraphs.html). For this we can
use the `inferno` tool (available via `cargo install`):

```sh
perf script | inferno-collapse-perf > stacks.folded
cat stacks.folded | inferno-flamegraph > flamegraph.svg
```

And that's it! You now have a flamegraph SVG file that can be opened and
navigated in your favorite web browser.

### Kubernetes

#### Kubernetes Dev Flow

There is a special flow for when you develop portions of Vector that are
designed to work with Kubernetes, like `kubernetes_logs` source or the
`deployment/kubernetes/*.yaml` configs.

This flow facilitates building Vector and deploying it into a cluster.

##### Requirements

There are some extra requirements besides what you'd normally need to work on
Vector:

- `linux` system (create an issue if you want to work with another OS and we'll
  help);
- [`skaffold`](https://skaffold.dev/)
- [`docker`](https://www.docker.com/)
- [`kubectl`](https://kubernetes.io/docs/tasks/tools/install-kubectl/)
- [`kustomize`](https://kustomize.io/)
- [`minikube`](https://minikube.sigs.k8s.io/)-powered or other k8s cluster
- [`cargo watch`](https://github.com/passcod/cargo-watch)

##### The dev flow

Once you have the requirements, use the `scripts/skaffold.sh dev` command.

That's it, just one command should take care of everything!

It will:

1. build the `vector` binary in development mode,
2. build a docker image from this binary via `skaffold/docker/Dockerfile`,
3. deploy `vector` into the Kubernetes cluster at your current kubectl context
   using the built docker image and a mix of our production deployment
   configuration from the `distribution/kubernetes/*.yaml` and the special
   dev-flow configuration at `skaffold/manifests/*.yaml`; see
   `kustomization.yaml` for the exact specification.

As the result of invoking the `scripts/skaffold.sh dev`, you should see
a `skaffold` process running on your local machine, printing the logs from the
deployed `vector` instance.

To stop the process, press `Ctrl+C`, and wait for `skaffold` to clean up
the cluster state and exit.

`scripts/skaffold.sh` wraps `skaffold`, you can use other `skaffold` subcommands
if it fits you better.

##### Troubleshooting

You might need to tweak `skaffold`, here are some hints:

- `skaffold` will try to detect whether a local cluster is used; if a local
  cluster is used, `skaffold` won't push the docker images it builds to a
  registry.
  See [this page](https://skaffold.dev/docs/environment/local-cluster/)
  for how you can troubleshoot and tweak this behavior.

- `skaffold` can rewrite the image name so that you don't try to push a docker
  image to a repo that you don't have access to.
  See [this page](https://skaffold.dev/docs/environment/image-registries/)
  for more info.

- For the rest of the `skaffold` tweaks you might want to apply check out
  [this page](https://skaffold.dev/docs/environment/).

##### Going through the dev flow manually

Is some cases `skaffold` may not work. It's possible to go through the dev flow
manually, without `skaffold`.

One of the important thing `skaffold` does is it patches the configuration to
tie things together. If you want to go without it, you'll have to take care of
that yourself, thus some additional knowledge of Kubernetes inner workings is
required.

Essentially, the steps you have to take to deploy manually are the same that
`skaffold` will perform, and they're outlined at the previous section.

#### Kubernetes E2E tests

Kubernetes integration has a lot of parts that can go wrong.

To cope with the complexity and ensure we maintain high quality, we use
E2E (end-to-end) tests.

> E2E tests normally run at CI, so there's typically no need to run them
> manually.

##### Requirements

- `kubernetes` cluster (`minikube` has special support, but any cluster should
  work)
- `docker`
- `kubectl`
- `bash`

Vector release artifacts are prepared for E2E tests, so the ability to do that
is required too, see Vector [docs](https://vector.dev) for more details.

> Note: `minikube` had a bug in the versions `1.12.x` that affected our test
> process - see https://github.com/kubernetes/minikube/issues/8799.
> Use version `1.13.0+` that has this bug fixed.

Also:

> Note: `minikube` has troubles running on ZFS systems. If you're using ZFS, we
> suggest using a cloud cluster or [`minik8s`](https://microk8s.io/) with local
> registry.

##### Running the E2E tests

To run the E2E tests, use the following command:

```shell
CONTAINER_IMAGE_REPO=<your name>/vector-test make test-e2e-kubernetes
```

Where `CONTAINER_IMAGE_REPO` is the docker image repo name to use, without part
after the `:`. Replace `<your name>` with your Docker Hub username.

You can also pass additional parameters to adjust the behavior of the test:

- `QUICK_BUILD=true` - use development build and a skaffold image from the dev
  flow instead of a production docker image. Significantly speeds up the
  preparation process, but doesn't guarantee the correctness in the release
  build. Useful for development of the tests or Vector code to speed up the
  iteration cycles.

- `USE_MINIKUBE_CACHE=true` - instead of pushing the built docker image to the
  registry under the specified name, directly load the image into
  a `minikube`-controlled cluster node.
  Requires you to test against a `minikube` cluster. Eliminates the need to have
  a registry to run tests.
  When `USE_MINIKUBE_CACHE=true` is set, we provide a default value for the
  `CONTAINER_IMAGE_REPO` so it can be omitted.
  Can be set to `auto` (default) to automatically detect whether to use
  `minikube cache` or not, based on the current `kubectl` context. To opt-out,
  set `USE_MINIKUBE_CACHE=false`.

- `CONTAINER_IMAGE=<your name>/vector-test:tag` - completely skip the step
  of building the Vector docker image, and use the specified image instead.
  Useful to speed up the iterations speed when you already have a Vector docker
  image you want to test against.

- `SKIP_CONTAINER_IMAGE_PUBLISHING=true` - completely skip the image publishing
  step. Useful when you want to speed up the iteration speed and when you know
  the Vector image you want to test is already available to the cluster you're
  testing against.

- `SCOPE` - pass a filter to the `cargo test` command to filter out the tests,
  effectively equivalent to `cargo test -- $SCOPE`.

Passing additional commands is done like so:

```shell
QUICK_BUILD=true USE_MINIKUBE_CACHE=true make test-e2e-kubernetes
```

or

```shell
QUICK_BUILD=true CONTAINER_IMAGE_REPO=<your name>/vector-test make test-e2e-kubernetes
```

## Humans

After making your change, you'll want to prepare it for Vector's users
(mostly humans). This usually entails updating documentation and announcing
your feature.

### Documentation

Documentation is very important to the Vector project! All documentation is
located in the `/docs` folder. To ensure your change is valid, you can run
`make check-docs`, which validates your changes to the `/docs` directory.

### Changelog

Developers do not need to maintain the [`Changelog`](/CHANGELOG.md). This is
automatically generated via the `make release` command. This is made possible
by the use of [conventional commit](#title) titles.

#### What makes a highlight noteworthy?

It should offer meaningful value to users. This is inherently subjective and
it is impossible to define exact rules for this distinction. But we should be
cautious not to dilute the meaning of a highlight by producing low values
highlights.

#### How is a highlight different from a blog post?

Highlights are not blog posts. They are short one, maybe two, paragraph
announcements. Highlights should allude to, or link to, a blog post if
relevant.

For example, [this performance increase announcement][urls.performance_highlight]
is noteworthy, but also deserves an in-depth blog post covering the work that
resulted in the performance benefit. Notice that the highlight alludes to an
upcoming blog post. This allows us to communicate a high-value performance
improvement without being blocked by an in-depth blog post.

## Security

Please see the [`SECURITY.md` file](/SECURITY.md).

## Legal

To protect all users of Vector, the following legal requirements are made.
If you have additional questions, please [contact us](#contact).

### DCO

Vector requires all contributors to agree to the DCO. DCO stands for Developer
Certificate of Origin and is maintained by the
[Linux Foundation](https://www.linuxfoundation.org). It is an attestation
attached to every commit made by every developer. All contributions are covered
by, and fall under, the DCO.

#### Trivial changes

Trivial changes, such as spelling fixes, do not need to be signed.

### Granted rights and copyright assignment

This is covered by the DCO. Contributions are covered by the DCO and do not
require a CLA.

## FAQ

### Why a DCO instead of a CLA?

It's simpler, clearer, and still protects users of Vector. We believe the DCO
more accurately embodies the principles of open-source. More info can be found
here:

- [Gitlab's switch to DCO](https://about.gitlab.com/2017/11/01/gitlab-switches-to-dco-license/)
- [DCO vs CLA](https://opensource.com/article/18/3/cla-vs-dco-whats-difference)

### If I’m contributing while an employee, do I still need my employer to sign something?

Nope! The DCO confirms that you are entitled to submit the code, which assumes
that you are authorized to do so. It treats you like an adult and relies on
your accurate statement about your rights to submit a contribution.

### What if I forgot to sign my commits?

No problem! We made this simple with the [`signoff` Makefile target](Makefile):

```bash
make signoff
```

If you prefer to do this manually:

```bash
git commit --amend --signoff
```

## Contact

If you have questions about this document or the project as a whole, please
contact us at vector@timber.io.

[urls.aws_announcements]: https://aws.amazon.com/new/?whats-new-content-all.sort-by=item.additionalFields.postDateTime&whats-new-content-all.sort-order=desc&wn-featured-announcements.sort-by=item.additionalFields.numericSort&wn-featured-announcements.sort-order=asc
[urls.create_branch]: https://help.github.com/en/github/collaborating-with-issues-and-pull-requests/creating-and-deleting-branches-within-your-repository
[urls.existing_issues]: https://github.com/timberio/vector/issues
[urls.fork_repo]: https://help.github.com/en/github/getting-started-with-github/fork-a-repo
[urls.github_sign_commits]: https://help.github.com/en/github/authenticating-to-github/signing-commits
[urls.new_issue]: https://github.com/timberio/vector/issues/new
[urls.push_it_to_the_limit]: https://www.youtube.com/watch?v=ueRzA9GUj9c
[urls.performance_highlight]: https://vector.dev/highlights/2020-04-11-overall-performance-increase
[urls.submit_pr]: https://help.github.com/en/github/collaborating-with-issues-and-pull-requests/creating-a-pull-request-from-a-fork
[urls.vector_test_harness]: https://github.com/timberio/vector-test-harness/
