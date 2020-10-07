#!/usr/bin/env bash

# This is a wrapper script around the system linker, which injects libgcc startup objects to allow
# statically linking Rust apps against C++ libraries while targeting musl.
#
# It should work as-is when using `cross` to cross-compile to musl. If not, it can be reconfigured
# via the environment variables shown below. Unfortunately it has to rely on a lot of hard-coded
# paths and arguments in order to do its job, so it may stop working when `cross` updates its Docker
# environment, or `rustc` changes the arguments it passes to the linker.
#
# Also see this link for compiler docs on which crt objects are used when:
# https://doc.rust-lang.org/nightly/nightly-rustc/rustc_target/spec/crt_objects/index.html
#
# Note that the name of this wrapper is significant: rustc automatically detects the "linker flavor"
# based on the linker executable name, and if it ends with `-ld` (which used to be the case), it
# will expect to invoke GNU LD directly. We want to use GCC instead, because it knows the search
# paths for locating the C++ runtime libraries. GCC is the default "flavor" on musl, so we just have
# to use a "neutral" wrapper name.

set -o errexit

# Object to inject after the predefined crt start objects.
INJECT_BEGIN=${RUST_MUSL_INJECT_BEGIN:-crtbeginS.o}

# Object to inject before the predefined crt end objects.
INJECT_END=${RUST_MUSL_INJECT_BEGIN:-crtendS.o}

# NB: We link the -S version of the objects because Rust produces position-independent executables.
# The non-S version fails to link in that case.

# The linker to forward to. Must accept GCC-style arguments (so must not be LD directly).
LINKER=''
if [ -x "$(command -v x86_64-linux-musl-gcc)" ]; then
    LINKER=x86_64-linux-musl-gcc
elif [ -x "$(command -v i686-linux-musl-gcc)" ]; then
    LINKER=i686-linux-musl-gcc
elif [ -x "$(command -v aarch64-linux-musl-gcc)" ]; then
    LINKER=aarch64-linux-musl-gcc
else
    LINKER=${RUST_MUSL_LINKER}
fi

args=("-l:$INJECT_BEGIN" "$@" "-l:$INJECT_END")

echo invoking real linker: "${LINKER}" "${args[@]}" >&2
"${LINKER}" "${args[@]}"
