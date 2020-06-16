# RFC 2685 - 2020-05-28 - Dev Workflow Simplification

Vector's `Makefile` serves a variety of purposes, and this RFC attempts to tame the complexity of common dev tasks, improving contributor and team member experience.

It proposes a practical base `environment` container/nix-env that merges the functionality of our non-integration test containers into one. It then suggests making common dev `make` tasks to rely on the caller environment having all dependencies, done at the same time it suggests adding `make` tasks to run common `make` tasks inside the environment. Finally, it suggests updating documentation to suggest users can use their native toolchains, `docker`, or `nix`.

It proposes these changes such that our team and contributors will have to explicitly **opt in** to virtualization/containerization/shell-manipulation, and attempts to do it in a convienent, unsurprising way.

## Motivation

Currently, Vector's build system is a wild, powerful beast with a lot of amazing features and mechanisms. Unfortunately, complexity has kind of gotten away from us, and we need to reclaim ownership of the automations.

Notably, our system works right now, as is, and we are not strongly motivated to replace or rebuild things just for the fun of it. We are motivated to make small, forward looking changes that improve the maintainability of our product.

**Before we do this, we should make sure we agree this offers realistic benefits to our future maintenance.**

## Guide-level Proposal

> **Note:** Replaces the **Development → Setup** section of the `[CONTRIBUTING.md](http://contributing.md)` file.

We're super excited to have you interested in working on Vector! Before you start you should pick how you want to develop.

For small or first-time contributions, we recommend the Docker method. If you do a lot of contributing, try adopting the Nix method! It'll be way faster and feel more smooth. Prefer to do it yourself? That's fine too!

### Use a Docker or Podman environment

> **Targets:** You can use this method to produce AARCH64, Arm6/7, as well as x86/64 Linux builds.

Since not everyone has a full working native environment, or can use Nix, we took our Nix environment and stuffed it into a Docker (or Podman) container!

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

Want to run from outside of the environment? Clever. You can run any of the following:

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
# Rebuild Vector's metadata
make generate ENVIRONMENT=true
# Serve the website on port 3000
make website ENVIRONMENT=true
# Format your code before pushing!
make fmt ENVIRONMENT=true
```

We use explicit environment opt-in as many contributors choose to keep their Rust toolchain local, and use `make generate ENVIRONMENT=true` etc.

### Using Nix

> **Note:** We're still new at this Nix stuff, so if you're a Nix expert do feel free to share knowledge.

If you're a Nix user, or you're open to trying out a new tool, you can use our Nix expressions!

If you don't have Nix yet, [install it](https://nixos.org/download.html):

```bash
curl -L [https://nixos.org/nix/install](https://nixos.org/nix/install) | sh
# Hesitating? Uninstalling is just `rm -rf /nix && rm -rf ~/.nix-*` then remove the import from `.bash_profile`
```

Next, run `nix-shell` from the Vector directory.

**_Wow, you did it!_** Now you can run the commands described in "Bring your own toolbox" now. This has pulled in all the required packages and set required environment variables.

Your other programs are there too, so you can run `code .` or `clion .` or whatever and get going like normal.

We've only partially adopted Nix to ensure contributors can still use familiar tools. You may still need to run `bundle install`, `yarn` or other commands to initialize things. (PRs welcome if you have ideas to improve this!)

If you're interested in having Nix **automatically start your environment**, you can consider a tool like [`direnv`](https://direnv.net/docs/installation.html), often available from your package manager.

```bash
❯ cd /git/timberio/vector
❯ direnv allow .
❯ cd ../vector
direnv: loading /git/timberio/vector/.envrc
direnv: using nix
cdirenv: export +AR +AS +CC +CONFIG_SHELL +CXX +HOST_PATH +IN_NIX_SHELL +LC_ALL +LD +NIX_BINTOOLS +NIX_BINTOOLS_WRAPPER_x86_64_unknown_linux_gnu_TARGET_HOST +NIX_BUILD_CORES +NIX_BUILD_TOP +NIX_CC +NIX_CC_WRAPPER_x86_64_unknown_linux_gnu_TARGET_HOST +NIX_CFLAGS_COMPILE +NIX_ENFORCE_NO_NATIVE +NIX_HARDENING_ENABLE +NIX_INDENT_MAKE +NIX_LDFLAGS +NIX_STORE +NM +OBJCOPY +OBJDUMP +PROTOC +PROTOC_INCLUDE +RANLIB +READELF +SIZE +SOURCE_DATE_EPOCH +STRINGS +STRIP +TEMP +TEMPDIR +TMP +TMPDIR +buildInputs +builder +configureFlags +depsBuildBuild +depsBuildBuildPropagated +depsBuildTarget +depsBuildTargetPropagated +depsHostHost +depsHostHostPropagated +depsTargetTarget +depsTargetTargetPropagated +doCheck +doInstallCheck +name +nativeBuildInputs +nobuildPhase +out +outputs +patches +phases +propagatedBuildInputs +propagatedNativeBuildInputs +shell +shellHook +stdenv +strictDeps +system ~LOCALE_ARCHIVE ~PATH
```

Now you can use the jobs detailed in **"Bring your own toolbox"** below.

### Bring your own toolbox

> **Targets:** This option is required for MSVC/Mac/FreeBSD toolchains.

To build Vector on your own host will require a fairly complete development environment!

We keep an up to date list of all dependencies used in our CI environment inside our `default.nix` file. Loosely, you'll need the following:

- **To build Vector:** Have working Rustup, C++/C build tools (LLVM, GCC, or MSVC), Python, and Perl, `make` (the GNU one preferably), `bash`, `cmake`, and `autotools`
- **To run integration tests:** Have `docker` available, or a real live version of that service.
- **To build the Website:** Have a working modern Ruby and Bundler toolchain available, also `bundle install` in the `scripts/` directory.
- **To run the Website in Dev:** Have a working `node` environment with `npm`/`yarn`, also run `yarn` from the `website/` directory.
- **To run `make check-component-features`:** Have `remarshal` installed.

If you find yourself needing to run something (such as `make generate`) inside the Docker environment described above, that's totally fine, they won't collide or hurt each other. In this case, you'd just run `make environment-generate`.

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
make test-integration SCOPE="sources::example" AUTOSPAWN=true
# Validate your tests pass against a live service.
make test-integration SCOPE="sources::example" AUTOSPAWN=false
cargo test --features docker sources::example
# Validate all tests pass (starting required services in Docker)
make test-integration
# Run your benchmarks
make bench SCOPE="transforms::example"
cargo bench transforms::example
# Rebuild Vector's metadata
make generate
# Serve the website on port 3000
make website
# Format your code before pushing!
make fmt
cargo fmt
```

If you run `make` you'll see a full list of all our tasks. Some of these will start Docker containers, sign commits, or even make releases. These are not common development commands and your mileage may vary.

## Doc-level Proposal

This change requires no User-facing docs changes. The changes to `[CONTRIBUTING.md](http://contributing.md)` are sufficient.

## Prior Art

Vector currently has an extensive `Makefile` and it does its job just fine. It is, unfortunately, rather confusing.

To help you picture it, common dev `make` tasks such as `make fmt` and `make check` do, roughly, the following:

- Start `make`, [calculate](https://github.com/timberio/vector/blob/1d8e88057f68d9cf9292ddc9edb69a7f8d3b3f92/Makefile#L7-L14) the default features
- [Run](https://github.com/timberio/vector/blob/1d8e88057f68d9cf9292ddc9edb69a7f8d3b3f92/Makefile#L3) the `/scripts/run.sh`. (via shebang this runs `env` which invokes `bash` )
- Run the `/scripts/prepare-target-dir.sh` script (via shebang, `env` then `bash`)
  - This `read` s then `grep`s the `docker-compose` yaml file, [running](https://github.com/timberio/vector/blob/1d8e88057f68d9cf9292ddc9edb69a7f8d3b3f92/scripts/prepare-target-dir.sh#L15-L17) `sed`, `sort`ing, then `uniq` ing the jobs
  - Make a directory as the current user for each of those.
- Run the `./scripts/docker-compose-run.sh` script (via shebang, `bash` , no `env` call)
  - Sets some env vars
  - Runs a `docker-compose rm` call to remove the existing service (this starts a `python` runtime, which dispatches to `docker`)
  - Runs `docker-compose up` on the given container.

At this point what happens differs by job. None of this is particularly slow, **it's just a lot.**

**It works fine.**

There are tools like `hab` (from the Habitat project) and `packer` that can be used to make containers as well!

## Sales Pitch

- Having an omnibus container means **we can build, publish, and cache the container**, and use it for CI or our users.
- This creates a **dependency test** since our CI will only ever have dependencies in the `default.nix` file, meaning we won't introduce accidental dependencies.
- This is **low effort**, hopefully lower effort than our current situation
- This is **not slower**, and should be faster for our team (as they can now use native toolchains with `make`, and if they want they can use `nix-shell` or `direnv`.
- We can **remove `docker-compose`** as a dependency.
- We can **better support Vector in Nixpkgs**.
- We can **remove a lot of tasks** from the `docker-compose` and begin cleaning out many scripts from `scripts/`
- We can **feel more comfortable editing our build** process.

## Drawbacks

- Let's face it, **Nix isn't awesome from a usability side**. `nix-env` and `nix-shell` have lots of warts, but so does every 20+ year old tech. It's a good tool to create and share reproducable environments, and that's what we're using it for.
- This adds **another 2 official dev envs, Nix and Native, instead of just Docker.** This means we may need to provide support.
- **Only Ana knows Nix right now**. But others can and have shown willingness to learn.
- It's **not backwards compatible**, meaning users who depend on the current `docker-compose` system might experience frustration.

## Outstanding Questions

- Windows/Mac/FreeBSD builds via `make build` et all will produce native binaries natively, we should be review those docs.
- This RFC does not scope in integration tests beyond letting the `environment` run them. We may find motivation to explore a more **_slick_** solution in the future.

## Rationale & Alternatives

Why change what we have?

- Running all these scripts and `docker` commands to run things like `cargo fmt` feels very... _stinky_... to experienced programmers.
- Most of our core team avoids the `Makefile` since it's been deemed not useful.
- We can reduce some duplication and redirection in the build system (hopefully understanding it better)
- We get side benefits like better dependency management.
- We can mature this system in fairly clever and optimized ways.

Alternatives:

- We could make the `environment` container in another packaging system/OS
- We could keep using our current system.
- We could explore merging some of our existing images but not provide an environment.

## Plan Of Attack

1. Introduce this RFC & a POC branch
2. Get preliminary consensus this is good path forward
3. Add `DOCKER_SOCKET` passing and support integration testing
4. Cross-OS testing, add `direnv` docs
5. Mature caching and nix-store management
6. Explore handling `make environment-%` commands via wildcard
7. Acceptance testing (Test including new contributor test)
8. Merge preliminary support
