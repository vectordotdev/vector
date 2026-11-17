# Linux container builds

Use `cargo vdev build linux <target>` to build Vector with native container tools.
The command replaces cross-rs. It uses the release feature set for the selected
Rust target and keeps the release profile, including fat LTO.

## Requirements

- Docker or Podman, with a local daemon on the same CPU architecture as the host.
- The repository's Rust toolchain, to run `cargo vdev`.
- An x86-64 host for x86-64 targets, or an ARM64 host for ARM64 and ARM32 targets.
- Enough memory and disk space for the complete release build. CI uses 8-core
  runners and allows up to 240 minutes.

ARM32 releases use **ARM64-hosted cross-compilers**. The compiler, linker, protoc,
and other build tools run as ARM64 programs. The command rejects a different host
architecture and QEMU binfmt registrations. Do not register QEMU to build these
targets.

## Targets and environments

| Rust target | Tool host | Build environment |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | x86-64 | AlmaLinux 8 |
| `aarch64-unknown-linux-gnu` | ARM64 | AlmaLinux 8 |
| `x86_64-unknown-linux-musl` | x86-64 | Alpine |
| `aarch64-unknown-linux-musl` | ARM64 | Alpine |
| `armv7-unknown-linux-gnueabihf` | ARM64 | Debian Bookworm, Zig glibc 2.17 sysroot |
| `arm-unknown-linux-gnueabi` | ARM64 | Debian Trixie, Zig glibc 2.17 sysroot |
| `armv7-unknown-linux-musleabihf` | ARM64 | Debian Bookworm, pinned musl/GCC toolchain |
| `arm-unknown-linux-musleabi` | ARM64 | Debian Bookworm, pinned musl/GCC toolchain |

GNU ARM release builds isolate target pkg-config paths and bundle zlib. The newer
GNU libraries in their build containers are used for runtime tests, not release
linking. Verification rejects GNU ARM binaries that require symbols newer than
glibc 2.17. The GNU 64-bit builder uses the same pinned AlmaLinux 8 base and SASL
settings as the existing native release job; its verification ceiling is 2.28.
Musl release verification rejects a dynamic interpreter or a dynamic dependency.

The independent [historical GNU ARM baseline](../tests/data/native-release-gnu-abi-baseline.json)
records the old image digests and measured requirements of the v0.61.0 release.
Both historical GNU ARM binaries require glibc 2.30. The compatibility check reads
the archived release and Makefile, not active build-image settings.

## Commands

On an x86-64 host, build and verify a release with:

```sh
cargo vdev build linux x86_64-unknown-linux-musl --verify
```

Use `--image-only` to prepare only the toolchain image. Use `--profile debug` for a
development build. `--command check`, `--command clippy`, and `--command test` use
the target toolchain in the container. GNU ARM test executables use the container's
GCC/runtime libraries; only `--command build` produces the compatible Zig-linked
release binary.

The Makefile provides equivalent entry points:

```sh
make native-build-x86_64-unknown-linux-musl
make NATIVE_VERIFY=true native-build-x86_64-unknown-linux-musl
make native-image-x86_64-unknown-linux-musl
```

Existing `cross-build-*`, `cross-test-*`, `cross-check-*`, `cross-clippy-*`, and
`cross-image-*` targets remain aliases. They do not install or invoke cross-rs.
The build isolates Cargo intermediates under `target/native-linux/<target>` so
host build scripts are not shared between libc environments. It copies the
executable to the existing `target/<target>/<profile>/vector` path and restores
output ownership before host packaging. Packaging with `PREBUILT=true` uses the
existing executable without compiling it again.

The Makefile uses `NATIVE_VDEV="cargo vdev"` for this new command, so a pinned
prebuilt `VDEV` remains usable for existing packaging tools. Override
`NATIVE_VDEV` only with a binary that includes `build linux`.

## ARMv6 runtime checks

Compiling ARMv6 does not require an ARMv6 execution environment. To execute the
probe, tests, or finished ARMv6 binary on an ARM64 Linux host, the kernel must
support native AArch32 execution and hardware CP15 barriers:

```sh
sudo sysctl -w abi.cp15_barrier=2
cargo vdev build linux arm-unknown-linux-gnueabi --probe-only
```

The default emulation mode can clear the exclusive monitor between LDREX and
STREX, which prevents atomic compare-exchange loops from making progress. The
builder checks this setting but does not change the host kernel. CI applies the
setting with `.github/actions/prepare-armv6-runtime` on an isolated ephemeral VM.
It does not change Vector, weaken atomics, or raise the ARMv6 CPU requirement.

## CI and publication

`.github/workflows/cross.yml` retains its public entry point and job names, but
uses the shared builder on matching host runners. It links all eight targets with
fat LTO and verifies their binaries. The release composite action uses the same
builder before hash-preserving archive and package creation.

Build and binary verification are not package installation or final-image
verification. The no-publish packaging workflow performs those checks separately.
QEMU is used only after compilation, for ARM1176 tests of completed ARMv6
production images. It is never registered with binfmt or used for build tools.

PR validation must keep `channel: custom`, `skip_publish: true`, `PUSH=false`, and
`LOAD=true`. Do not publish PR packages, images, or releases.
