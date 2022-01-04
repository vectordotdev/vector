#!/bin/bash

# install.sh
#
# This is just a little script that can be downloaded from the internet to
# install Vector. It just does platform detection and execute the appropriate
# install method.
#
# Heavily inspired by the Rustup script at https://sh.rustup.rs

set -u

# If PACKAGE_ROOT is unset or empty, default it.
PACKAGE_ROOT="${PACKAGE_ROOT:-"https://packages.timber.io/vector"}"
VECTOR_VERSION="0.19.0"
_divider="--------------------------------------------------------------------------------"
_prompt=">>>"
_indent="   "

header() {
    cat 1>&2 <<EOF
                                   __   __  __
                                   \ \ / / / /
                                    \ V / / /
                                     \_/  \/

                                   V E C T O R
                                    Installer


$_divider
Website: https://vector.dev
Docs: https://vector.dev/docs/
Community: https://vector.dev/community/
$_divider

EOF
}

usage() {
    cat 1>&2 <<EOF
vector-install
The installer for Vector (https://vector.dev)

USAGE:
    vector-install [FLAGS] [OPTIONS]

FLAGS:
    -y                      Disable confirmation prompt.
        --no-modify-path    Don't configure the PATH environment variable
    -h, --help              Prints help information
EOF
}

main() {
    downloader --check
    header

    local prompt=yes
    local modify_path=yes
    for arg in "$@"; do
        case "$arg" in
            -h|--help)
                usage
                exit 0
                ;;
            --no-modify-path)
                modify_path=no
                ;;
            -y)
                prompt=no
                ;;
            *)
                ;;
        esac
    done

    # Confirm with the user before proceeding to install Vector through a
    # package manager. Otherwise, we install from an archive.
    if [ "$prompt" = "yes" ]; then
        echo "$_prompt We'll be installing Vector via a pre-built archive at https://packages.timber.io/vector/${VECTOR_VERSION}/"
        echo "$_prompt Ready to proceed? (y/n)"
        echo ""

        while true; do
            read -rp "$_prompt " _choice </dev/tty
            case $_choice in
                n)
                    err "exiting"
                    ;;
                y)
                    break
                    ;;
                *)
                    echo "Please enter y or n."
                    ;;
            esac
        done

        # Print a divider to separate the Vector installer output and the
        # package manager installer output.
        echo ""
        echo "$_divider"
        echo ""
    fi

    install_from_archive $modify_path
}

install_from_archive() {
    need_cmd cp
    need_cmd mktemp
    need_cmd mkdir
    need_cmd rm
    need_cmd rmdir
    need_cmd grep
    need_cmd tar
    need_cmd head
    need_cmd sed

    get_architecture || return 1
    local modify_path="$1"
    local _arch="$RETVAL"
    assert_nz "$_arch" "arch"

    local _archive_arch=""
    case "$_arch" in
        x86_64-apple-darwin)
            _archive_arch=$_arch
            ;;
        x86_64-*linux*-gnu)
            _archive_arch="x86_64-unknown-linux-gnu"
            ;;
        x86_64-*linux*-musl)
            _archive_arch="x86_64-unknown-linux-musl"
            ;;
        aarch64-*linux*)
            _archive_arch="aarch64-unknown-linux-musl"
            ;;
	armv7-*linux*-gnu)
            _archive_arch="armv7-unknown-linux-gnueabihf"
            ;;
	armv7-*linux*-musl)
            _archive_arch="armv7-unknown-linux-musleabihf"
            ;;
        *)
            err "unsupported arch: $_arch"
            ;;
    esac

    local _url="${PACKAGE_ROOT}/${VECTOR_VERSION}/vector-${VECTOR_VERSION}-${_archive_arch}.tar.gz"

    local _dir
    _dir="$(mktemp -d 2>/dev/null || ensure mktemp -d -t vector-install)"

    local _file="${_dir}/vector-${VECTOR_VERSION}-${_archive_arch}.tar.gz"

    ensure mkdir -p "$_dir"

    printf "%s Downloading Vector via %s" "$_prompt" "$_url"
    ensure downloader "$_url" "$_file"
    printf " ✓\n"

    printf "%s Unpacking archive to $HOME/.vector ..." "$_prompt"
    ensure mkdir -p "$HOME/.vector"
    ensure tar -xzf "$_file" --directory="$HOME/.vector" --strip-components=2

    printf " ✓\n"

    if [ "$modify_path" = "yes" ]; then
      local _path="export PATH=\"\$HOME/.vector/bin:\$PATH\""
      add_to_path "${HOME}/.zprofile" "${_path}"
      add_to_path "${HOME}/.profile" "${_path}"
      printf " ✓\n"
    fi

    printf "%s Install succeeded! 🚀\n" "$_prompt"
    printf "%s To start Vector:\n" "$_prompt"
    printf "\n"
    printf "%s vector --config ~/.vector/config/vector.toml\n" "$_indent"
    printf "\n"
    printf "%s More information at https://vector.dev/docs/\n" "$_prompt"

    local _retval=$?

    ignore rm "$_file"
    ignore rmdir "$_dir"

    return "$_retval"
}

add_to_path() {
  local file="$1"
  local new_path="$2"

  printf "%s Adding Vector path to ${file}" "$_prompt"

  if [ ! -f "$file" ]; then
    echo "${new_path}" >> "${file}"
  else
    grep -qxF "${new_path}" "${file}" || echo "${new_path}" >> "${file}"
  fi
}

# ------------------------------------------------------------------------------
# All code below here was copied from https://sh.rustup.rs and can safely
# be updated if necessary.
# ------------------------------------------------------------------------------

get_gnu_musl_glibc() {
  need_cmd ldd
  need_cmd bc
  need_cmd awk
  # Detect both gnu and musl
  # Also detect glibc versions older than 2.18 and return musl for these
  # Required until we address https://github.com/timberio/vector/issues/5412.
  local _ldd_version
  local _glibc_version
  _ldd_version=$(ldd --version)
  if [[ $_ldd_version =~ "GNU" ]]; then
    _glibc_version=$(echo "$_ldd_version" | awk '/ldd/{print $NF}')
    if [ 1 -eq "$(echo "${_glibc_version} < 2.18" | bc)" ]; then
      echo "musl"
    else
      echo "gnu"
    fi
elif [[ $_ldd_version =~ "musl" ]]; then
  echo "musl"
else
  err "Unknown architecture from ldd: ${_ldd_version}"
fi
}

get_bitness() {
    need_cmd head
    # Architecture detection without dependencies beyond coreutils.
    # ELF files start out "\x7fELF", and the following byte is
    #   0x01 for 32-bit and
    #   0x02 for 64-bit.
    # The printf builtin on some shells like dash only supports octal
    # escape sequences, so we use those.
    local _current_exe_head
    _current_exe_head=$(head -c 5 /proc/self/exe )
    if [ "$_current_exe_head" = "$(printf '\177ELF\001')" ]; then
        echo 32
    elif [ "$_current_exe_head" = "$(printf '\177ELF\002')" ]; then
        echo 64
    else
        err "unknown platform bitness"
    fi
}

get_endianness() {
    local cputype=$1
    local suffix_eb=$2
    local suffix_el=$3

    # detect endianness without od/hexdump, like get_bitness() does.
    need_cmd head
    need_cmd tail

    local _current_exe_endianness
    _current_exe_endianness="$(head -c 6 /proc/self/exe | tail -c 1)"
    if [ "$_current_exe_endianness" = "$(printf '\001')" ]; then
        echo "${cputype}${suffix_el}"
    elif [ "$_current_exe_endianness" = "$(printf '\002')" ]; then
        echo "${cputype}${suffix_eb}"
    else
        err "unknown platform endianness"
    fi
}

get_architecture() {
    local _ostype _cputype _bitness _arch
    _ostype="$(uname -s)"
    _cputype="$(uname -m)"

    if [ "$_ostype" = Linux ]; then
        if [ "$(uname -o)" = Android ]; then
            _ostype=Android
        fi
    fi

    if [ "$_ostype" = Darwin ] && [ "$_cputype" = i386 ]; then
        # Darwin `uname -m` lies
        if sysctl hw.optional.x86_64 | grep -q ': 1'; then
            _cputype=x86_64
        fi
    fi

    case "$_ostype" in

        Android)
            _ostype=linux-android
            ;;

        Linux)
            case $(get_gnu_musl_glibc) in
              "musl")
                _ostype=unknown-linux-musl
                ;;
              "gnu")
                _ostype=unknown-linux-gnu
                ;;
              # Fallback
              *)
                _ostype=unknown-linux-gnu
                ;;
            esac
            _bitness=$(get_bitness)
            ;;

        FreeBSD)
            _ostype=unknown-freebsd
            ;;

        NetBSD)
            _ostype=unknown-netbsd
            ;;

        DragonFly)
            _ostype=unknown-dragonfly
            ;;

        Darwin)
            _ostype=apple-darwin
            ;;

        MINGW* | MSYS* | CYGWIN*)
            _ostype=pc-windows-gnu
            ;;

        *)
            err "unrecognized OS type: $_ostype"
            ;;

    esac

    case "$_cputype" in

        i386 | i486 | i686 | i786 | x86)
            _cputype=i686
            ;;

        xscale | arm)
            _cputype=arm
            if [ "$_ostype" = "linux-android" ]; then
                _ostype=linux-androideabi
            fi
            ;;

        armv6l)
            _cputype=arm
            if [ "$_ostype" = "linux-android" ]; then
                _ostype=linux-androideabi
            else
                _ostype="${_ostype}eabihf"
            fi
            ;;

        armv7l | armv8l)
            _cputype=armv7
            if [ "$_ostype" = "linux-android" ]; then
                _ostype=linux-androideabi
            else
                _ostype="${_ostype}eabihf"
            fi
            ;;

        aarch64)
            _cputype=aarch64
            ;;

        x86_64 | x86-64 | x64 | amd64)
            _cputype=x86_64
            ;;

        mips)
            _cputype=$(get_endianness mips '' el)
            ;;

        mips64)
            if [ "$_bitness" -eq 64 ]; then
                # only n64 ABI is supported for now
                _ostype="${_ostype}abi64"
                _cputype=$(get_endianness mips64 '' el)
            fi
            ;;

        ppc)
            _cputype=powerpc
            ;;

        ppc64)
            _cputype=powerpc64
            ;;

        ppc64le)
            _cputype=powerpc64le
            ;;

        s390x)
            _cputype=s390x
            ;;

        *)
            err "unknown CPU type: $_cputype"

    esac

    # Detect 64-bit linux with 32-bit userland
    if [ "${_ostype}" = unknown-linux-gnu ] && [ "${_bitness}" -eq 32 ]; then
        case $_cputype in
            x86_64)
                _cputype=i686
                ;;
            mips64)
                _cputype=$(get_endianness mips '' el)
                ;;
            powerpc64)
                _cputype=powerpc
                ;;
            aarch64)
                _cputype=armv7
                if [ "$_ostype" = "linux-android" ]; then
                    _ostype=linux-androideabi
                else
                    _ostype="${_ostype}eabihf"
                fi
                ;;
        esac
    fi

    # Detect armv7 but without the CPU features Rust needs in that build,
    # and fall back to arm.
    # See https://github.com/rust-lang/rustup.rs/issues/587.
    if [ "$_ostype" = "unknown-linux-gnueabihf" ] && [ "$_cputype" = armv7 ]; then
        if ensure grep '^Features' /proc/cpuinfo | grep -q -v neon; then
            # At least one processor does not have NEON.
            _cputype=arm
        fi
    fi

    _arch="${_cputype}-${_ostype}"

    RETVAL="$_arch"
}

err() {
    echo "$_prompt $1" >&2
    exit 1
}

need_cmd() {
    if ! check_cmd "$1"; then
        err "need '$1' (command not found)"
    fi
}

check_cmd() {
    command -v "$1" > /dev/null 2>&1
}

assert_nz() {
    if [ -z "$1" ]; then err "assert_nz $2"; fi
}

# Run a command that should never fail. If the command fails execution
# will immediately terminate with an error showing the failing
# command.
ensure() {
    local output
    output="$("$@" 2>&1 > /dev/null)"

    if [ "$output" ]; then
        echo ""
        echo "$_prompt command failed: $*"
        echo ""
        echo "$_divider"
        echo ""
        echo "$output" >&2
        exit 1
    fi
}

# This is just for indicating that commands' results are being
# intentionally ignored. Usually, because it's being executed
# as part of error handling.
ignore() {
    "$@"
}

# This wraps curl or wget. Try curl first, if not installed,
# use wget instead.
downloader() {
    local _dld
    if check_cmd curl; then
        _dld=curl
    elif check_cmd wget; then
        _dld=wget
    else
        _dld='curl or wget' # to be used in error message of need_cmd
    fi

    if [ "$1" = --check ]; then
        need_cmd "$_dld"
    elif [ "$_dld" = curl ]; then
        if ! check_help_for curl --proto --tlsv1.2; then
            echo "Warning: Not forcing TLS v1.2, this is potentially less secure"
            curl --silent --show-error --fail --location "$1" --output "$2"
        else
            curl --proto '=https' --tlsv1.2 --silent --show-error --fail --location "$1" --output "$2"
        fi
    elif [ "$_dld" = wget ]; then
        if ! check_help_for wget --https-only --secure-protocol; then
            echo "Warning: Not forcing TLS v1.2, this is potentially less secure"
            wget "$1" -O "$2"
        else
            wget --https-only --secure-protocol=TLSv1_2 "$1" -O "$2"
        fi
    else
        err "Unknown downloader"   # should not reach here
    fi
}

check_help_for() {
    local _cmd
    local _arg
    local _ok
    _cmd="$1"
    _ok="y"
    shift

    # If we're running on OS-X, older than 10.13, then we always
    # fail to find these options to force fallback
    if check_cmd sw_vers; then
        if [ "$(sw_vers -productVersion | cut -d. -f2)" -lt 13 ]; then
            # Older than 10.13
            echo "Warning: Detected OS X platform older than 10.13"
            _ok="n"
        fi
    fi

    for _arg in "$@"; do
        if ! "$_cmd" --help | grep -q -- "$_arg"; then
            _ok="n"
        fi
    done

    test "$_ok" = "y"
}

main "$@" || exit 1
