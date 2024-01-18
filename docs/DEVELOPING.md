# Developing

- [Setup](#setup)
  - [Using a Docker or Podman environment](#using-a-docker-or-podman-environment)
  - [Bring your own toolbox](#bring-your-own-toolbox)
- [The basics](#the-basics)
  - [Directory structure](#directory-structure)
  - [Makefile](#makefile)
  - [Code style](#code-style)
    - [Logging style](#logging-style)
    - [Panics](#panics)
  - [Feature flags](#feature-flags)
  - [Dependencies](#dependencies)
  - [Minimum Supported Rust Version](#minimum-supported-rust-version)
- [Guidelines](#guidelines)
  - [Sink healthchecks](#sink-healthchecks)
- [Testing](#testing)
  - [Unit tests](#unit-tests)
  - [Integration tests](#integration-tests)
  - [Blackbox tests](#blackbox-tests)
  - [Property tests](#property-tests)
  - [Tips and tricks](#tips-and-tricks)
    - [Faster builds With `sccache`](#faster-builds-with-sccache)
    - [Testing specific components](#testing-specific-components)
    - [Generating sample logs](#generating-sample-logs)
- [Benchmarking](#benchmarking)
- [Profiling](#profiling)
- [Domains](#domains)
  - [Kubernetes](#kubernetes)
    - [Architecture](#architecture)
      - [The operation logic](#the-operation-logic)
      - [Where to find things](#where-to-find-things)
    - [Development](#development)
      - [Requirements](#requirements)
      - [Automatic](#automatic)
    - [Testing](#testing-1)
      - [Integration tests](#integration-tests-1)
        - [Requirements](#requirements-1)
        - [Tutorial](#tutorial)

## Setup

We're super excited to have you interested in working on Vector! Before you start you should pick how you want to develop.

For small or first-time contributions, we recommend the Docker method. Prefer to do it yourself? That's fine too!

### Using a Docker or Podman environment

> **Targets:** You can use this method to produce AARCH64, Arm6/7, as well as x86/64 Linux builds.

Since not everyone has a full working native environment, we took our environment and stuffed it into a Docker (or Podman) container!

This is ideal for users who want it to "Just work" and just want to start contributing. It's also what we use for our CI, so you know if it breaks we can't do anything else until we fix it. 😉

**Before you go further, install Docker or Podman through your official package manager, or from the [Docker](https://docs.docker.com/get-docker/) or [Podman](https://podman.io/) sites.**

```bash
# Optional: Only if you use `podman`
export CONTAINER_TOOL="podman"
```

If your Linux environment runs SELinux in Enforcing mode, you will need to relabel the vector source code checkout with `container_home_t` context. Otherwise, the container environment cannot read/write the code:

```bash
cd your/checkout/of/vector/
sudo semanage fcontext -a "${PWD}(/.*)?" -t container_file_t
sudo restorecon . -R
```

By default, `make environment` style tasks will do a `docker pull` from GitHub's container repository, you can **optionally** build your own environment while you make your morning coffee ☕:

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

### Bring your own toolbox

> **Targets:** This option is required for MSVC/Mac/FreeBSD toolchains. It can be used to build for any environment or OS.

To build Vector on your own host will require a fairly complete development environment!

Loosely, you'll need the following:

- **To build Vector:** Have working Rustup, Protobuf tools, C++/C build tools (LLVM, GCC, or MSVC), Python, and Perl, `make` (the GNU one preferably), `bash`, `cmake`, `GNU coreutils`, and `autotools`.
- **To run `make test`:** Install [`cargo-nextest`](https://nexte.st/)
- **To run integration tests:** Have `docker` available, or a real live version of that service. (Use `AUTOSPAWN=false`)
- **To run `make check-component-features`:** Have `remarshal` installed.
- **To run `make check-licenses` or `cargo vdev build licenses`:** Have `dd-rust-license-tool` [installed](https://github.com/DataDog/rust-license-tool).
- **To run `cargo vdev build component-docs`:** Have `cue` [installed](https://cuelang.org/docs/install/).

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
make test SCOPE="sources::example"
# Validate tests (that do not require other services) pass
cargo test
make test
# Validate your tests pass (starting required services in Docker)
make test-integration SCOPE="sources::example"
# Validate your tests pass against a live service.
make test-integration SCOPE="sources::example" autospawn=false
cargo test --features docker sources::example
# Validate all tests pass (starting required services in Docker)
make test-integration
# Run your benchmarks
make bench SCOPE="transforms::example"
cargo bench transforms::example
# Format your code before pushing!
make fmt
cargo fmt
# Build component documentation for the website
cargo vdev build component-docs
```

If you run `make` you'll see a full list of all our tasks. Some of these will start Docker containers, sign commits, or even make releases. These are not common development commands and your mileage may vary.

## The basics

### Directory structure

- [`/.github`](../.github) - GitHub & CI related configuration.
- [`/benches`](../benches) - Internal benchmarks.
- [`/config`](../config) - Public facing Vector config, included in releases.
- [`/distribution`](../distribution) - Distribution artifacts for various targets.
- [`/docs`](../docs) - Internal documentation for Vector contributors.
- [`/lib`](../lib) - External libraries that do not depend on `vector` but are used within the project.
- [`/proto`](../proto) - Protobuf definitions.
- [`/rfcs`](../rfcs) - Previous Vector proposals, a great place to build context on previous decisions.
- [`/scripts`](../scripts) - Scripts used to generate docs and maintain the repo.
- [`/src`](../src) - Vector source.
- [`/tests`](../tests) - Various high-level test cases.
- [`/website`](../website) - Vector's website and external documentation for Vector users.

### Makefile

Vector includes a [`Makefile`](../Makefile) in the root of the repo. This serves
as a high-level interface for common commands. Running `make` will produce
a list of make targets with descriptions. These targets will be referenced
throughout this document.

### Code style

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

#### Logging style

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

#### Panics

As a general rule, code in Vector should *not* panic.

However, there are very rare situations where the code makes certain assumptions
about the given state and if those assumptions are not met this is clearly due
to a bug within Vector. In this situation Vector cannot safely proceed. Issuing
a panic here is acceptable.

All potential panics *MUST* be clearly documented in the function documentation.

### Feature flags

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

### Dependencies

Dependencies should be _carefully_ selected and avoided if possible. You can
see how dependencies are reviewed in the
[Reviewing guide](/docs/REVIEWING.md#dependencies).

If a dependency is required only by one or multiple components, but not by
Vector's core, make it optional and add it to the list of dependencies of
the features corresponding to these components in `Cargo.toml`.

### Minimum Supported Rust Version

Vector's Minimum Supported Rust Version (MSRV) is indicated by the `rust-version` specified in
`Cargo.toml`.

Currently, Vector has no policy around MSRV. It can be bumped at any time if required by
a dependency or to take advantage of a new language feature in Vector's codebase.

## Guidelines

### Sink healthchecks

Sinks may implement a health check as a means for validating their configuration
against the environment and external systems. Ideally, this allows the system to
inform users of problems such as insufficient credentials, unreachable
endpoints, nonexistent tables, etc. They're not perfect, however, since it's
impossible to exhaustively check for issues that may happen at runtime.

When implementing health checks, we prefer false positives to false negatives.
This means we would prefer that a health check pass and the sink then fail than
to have the health check fail when the sink would have been able to run
successfully.

A common cause of false negatives in health checks is performing an operation
that the sink itself does not need. For example, listing all the available S3
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
cases, random test data is reasonably likely to trigger a potential
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

Not all the answers need to be a hard "no", but we should think about the
likelihood that any "yes" would lead to false negatives and balance that against
the usefulness of the check as a whole for finding problems. Because we have the
option to disable individual health checks, there's an escape hatch for users
that fall into a false negative circumstance. Our goal should be to minimize the
likelihood of users needing to pull that lever while still making a good effort
to detect common problems.

## Testing

Testing is very important since Vector's primary design principle is reliability.
You can read more about how Vector tests in our
[testing blog post](https://vector.dev/blog/how-we-test-vector/).

### Unit tests

Unit tests refer to the majority of inline tests throughout Vector's code. A
defining characteristic of unit tests is that they do not require external
services to run, therefore they should be much quicker. You can run them with:

```bash
cargo test
```

### Integration tests

Integration tests verify that Vector actually works with the services it
integrates with. Unlike unit tests, integration tests require external services
to run. A few rules when setting up integration tests:

- [ ] To ensure all contributors can run integration tests, the service must
      run in a Docker container.
- [ ] The service must be configured on a unique port that is configured through
      an environment variable.
- [ ] Add a `test-integration-<name>` to Vector's [`Makefile`](/Makefile) and
      ensure that it starts the service before running the integration test.
- [ ] Add the name of your integration to the include matrix of the `test-integration` job to Vector's
      [`.github/workflows/integration-test.yml`](../.github/workflows/integration-test.yml) workflow.

Once complete, you can run your integration tests with:

```bash
make test-integration-<name>
```

### Blackbox tests

Vector also offers blackbox testing via
[Vector's test harness](https://github.com/vectordotdev/vector-test-harness). This
is a complex testing suite that tests Vector's performance in real-world
environments. It is typically used for benchmarking, but also correctness
testing.

You can run these tests within a PR as described in the [CI section](CONTRIBUTING.md).

### Property tests

Vector prefers the use of [Proptest](https://github.com/proptest-rs/proptest) for any property tests.

### Tips and tricks

#### Faster builds With `sccache`

Vector is a large project with a plethora of dependencies. Changing to a different branch, or
running `cargo clean`, can sometimes necessitate rebuilding many of those dependencies, which has an
impact on productivity. One way to reduce some of this cycle time is to use `sccache`, which caches
compilation assets to avoid recompiling them over and over.

`sccache` works by being configured to sit in front of `rustc`, taking compilation requests from
Cargo and checking the cache to see if it already has the cached compilation unit. It handles
making sure that different compiler flags, versions of Rust, etc., are taken into consideration
before using a cached asset.

In order to use `sccache`, you must first [install](https://github.com/mozilla/sccache#installation)
it. There are pre-built binaries for all major platforms to get you going quickly. The
[usage](https://github.com/mozilla/sccache#usage) documentation also explains how to set up your
environment to actually use it. We recommend using the `$HOME/.cargo/config` approach as this can help
speed up all of your Rust development work, and not just developing on Vector.

While `sccache` was originally designed to cache compilation assets in cloud storage, maximizing
reusability amongst CI workers, `sccache` actually supports storing assets locally by default.
Local mode works well for local development as it is much easier to delete the cache directory if
you ever encounter issues with the cached assets. It also involves no extra infrastructure or
spending.

#### Testing specific components

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
     'cargo test --lib --no-default-features --features=<component type>-<component id> <component type>::<component id>'
   ```

   For example, if the component is `reduce` transform, the command above
   turns into

   ```sh
   cargo watch -s clear -s \
     'cargo test --lib --no-default-features --features=transforms-reduce transforms::reduce'
   ```

#### Generating sample logs

We use `flog` to build a sample set of log files to test sending logs from a
file. This can be done with the following commands on Mac with `homebrew`.
Installation instruction for flog can be found
[here](https://github.com/mingrammer/flog#installation).

```bash
flog --bytes $((100 * 1024 * 1024)) > sample.log
```

This will create a `100MiB` sample log file in the `sample.log` file.

## Benchmarking

All benchmarks are placed in the [`/benches`](/benches) folder. You can
run benchmarks via the `make bench` command. In addition, Vector
maintains a full [test harness](https://github.com/vectordotdev/vector-test-harness)
for complex end-to-end integration and performance testing.

## Profiling

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

You'll now find a `perf.data` file in your current directory with all the
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

## Domains

This section contains domain specific development knowledge for various areas
of Vector. You should scan this section for any relevant domains for your
development area.

### Kubernetes

#### Architecture

The Kubernetes integration architecture is largely inspired by
the [RFC 2221](../rfcs/2020-04-04-2221-kubernetes-integration.md), so this
is a concise outline of the effective design, rather than a deep dive into
the concepts.

##### The operation logic

With `kubernetes_logs` source, Vector connects to the Kubernetes API doing
a streaming watch request over the `Pod`s executing on the same `Node` that
Vector itself runs at. Once Vector gets the list of all the `Pod`s that are
running on the `Node`, it starts collecting logs for the logs files
corresponding to each of the `Pod`. Only plaintext (as in non-gzipped) files
are taken into consideration.
The log files are then parsed into events, and the said events are annotated
with the metadata from the corresponding `Pod`s, correlated via the file path
of the originating log file.
The events are then passed to the topology.

##### Where to find things

We use custom Kubernetes API client and machinery, that lives
at `src/kubernetes`.
The `kubernetes_logs` source lives at `src/sources/kubernetes_logs`.
There is also an end-to-end (E2E) test framework that resides
at `lib/k8s-test-framework`, and the actual end-to-end tests using that
framework are at `lib/k8s-e2e-tests`.

The Kubernetes-related distribution bit that are at `distribution/docker`,
`distribution/kubernetes` and our Helm chart can be found at [`vectordotdev/helm-charts`](https://github.com/vectordotdev/helm-charts/).

The development assistance resources are located at `Tiltfile`
and in the `tilt` dir.

#### Development

There is a special flow for when you develop portions of Vector that are
designed to work with Kubernetes, like `kubernetes_logs` source or the
`deployment/kubernetes/*.yaml` configs.

This flow facilitates building Vector and deploying it into a cluster.

##### Requirements

There are some extra requirements besides what you'd normally need to work on
Vector:

- [`tilt`](https://tilt.dev/)
- [`docker`](https://www.docker.com/)
- [`kubectl`](https://kubernetes.io/docs/tasks/tools/install-kubectl/)
- [`minikube`](https://minikube.sigs.k8s.io/)-powered or other k8s cluster

##### Automatic

You can use `tilt` to detect changes, rebuild your image, and update your
Kubernetes resource. Simply start your local Kubernetes cluster and run
`tilt up` from Vector's root dir.

#### Testing

##### Integration tests

The Kubernetes integration tests have a lot of parts that can go wrong.

To cope with the complexity and ensure we maintain high quality, we use
E2E (end-to-end) tests.

> E2E tests normally run at CI, so there's typically no need to run them
> manually.

###### Requirements

- `kubernetes` cluster (`minikube` has special support, but any cluster should
  work)
- `docker`
- `kubectl`
- `bash`
- `cross` - `cargo install cross`
- [`helm`](https://helm.sh/)

Vector release artifacts are prepared for E2E tests, so the ability to do that
is required too, see Vector [docs](https://vector.dev) for more details.

Notes:

> - `minikube` had a bug in the versions `1.12.x` that affected our test
>   process - see <https://github.com/kubernetes/minikube/issues/8799>.
>   Use version `1.13.0+` that has this bug fixed.
> - `minikube` has troubles running on ZFS systems. If you're using ZFS, we
>   suggest using a cloud cluster or [`minik8s`](https://microk8s.io/) with local
>   registry.
> - E2E tests expect to have enough resources to perform a full Vector build,
>   usually 8GB of RAM with 2CPUs are sufficient to successfully complete E2E tests
>   locally.

###### Tutorial

To run the E2E tests, use the following command:

```shell
CONTAINER_IMAGE_REPO=<your name>/vector-test make test-e2e-kubernetes
```

Where `CONTAINER_IMAGE_REPO` is the docker image repo name to use, without part
after the `:`. Replace `<your name>` with your Docker Hub username.

You can also pass additional parameters to adjust the behavior of the test:

- `QUICK_BUILD=true` - use development build and an image from the dev
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
