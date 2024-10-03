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
# If VECTOR_VERSION is unset or empty, default it.
VECTOR_VERSION="${VECTOR_VERSION:-"0.41.1"}"
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
        --prefix            The directory where the files should be placed, default: "$HOME/.vector"
                            Note: This option automatically assumes the \`--no-modify-path\` flag
        --no-modify-path    Don't configure the PATH environment variable
    -h, --help              Prints help information
EOF
}

main() {
    downloader --check
    header

    local prompt=yes
    local modify_path=yes
    local prefix="$HOME/.vector"
    local use_new_directory_structure=no
    for arg in "$@"; do
        case "$arg" in
            -h|--help)
                usage
                exit 0
                ;;
            --prefix)
                prefix="$2"
                use_new_directory_structure=yes
                modify_path=no
                shift 2
                ;;
            --no-modify-path)
                modify_path=no
                shift
                ;;
            -y)
                prompt=no
                shift
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

    install_from_archive "$modify_path" "$prefix" "$use_new_directory_structure"
}

install_from_archive() {
    need_cmd dirname
    need_cmd pwd
    need_cmd basename
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
    local prefix="$2"
    local use_new_directory_structure="$3"
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
        aarch64-apple-darwin)
            # This if statement can be removed when Vector publishes aarch64-apple-darwin builds
            if /usr/bin/pgrep oahd >/dev/null 2>&1; then
                echo "Rosetta is installed, installing x86_64-apple-darwin archive"
                _archive_arch="x86_64-apple-darwin"
            else
                echo "Builds for Apple Silicon are not published today, please install Rosetta"
                err "unsupported arch: $_arch"
            fi
            ;;
        aarch64-*linux*)
            _archive_arch="aarch64-unknown-linux-musl"
            ;;
        armv7-*linux*-gnueabihf)
            _archive_arch="armv7-unknown-linux-gnueabihf"
            ;;
        armv7-*linux*-musleabihf)
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
    ensure downloader "$_url" "$_file" "$_arch"
    printf " ✓\n"

    ensure mkdir -p "$prefix"

    if [ "$use_new_directory_structure" = "no" ]; then
        printf "%s Unpacking archive to $prefix ..." "$_prompt"
        ensure tar -xzf "$_file" --directory="$prefix" --strip-components=2
    else
        # https://github.com/vectordotdev/vector/pull/13613#pullrequestreview-1045524132.
        # We will unpack the archive to a temporary directory and then copy the files to
        # their corresponding locations according to the new directory structure.
        printf "%s Using new directory structure since --prefix was specified...\n" "$_prompt"
        local _unpack_dir
        _unpack_dir="$(mktemp -d 2>/dev/null || ensure mktemp -d -t vector-install)"
        ensure tar -xzf "$_file" --directory="$_unpack_dir" --strip-components=2
        # copy all files (including hidden), ref: https://askubuntu.com/a/86891
        ensure cp -r "$_unpack_dir/bin/." "$prefix/bin"
        ensure cp -r "$_unpack_dir/etc/." "$prefix/etc"
        ensure mkdir -p "$prefix/share/vector/config"
        ensure cp -r "$_unpack_dir/config/." "$prefix/share/vector/config"
        ensure cp "$_unpack_dir"/README.md "$prefix/share/vector/"
        ensure cp "$_unpack_dir"/LICENSE "$prefix/share/vector/"
        # all files have been moved, we can safely remove the unpack directory
        ignore rm -rf "$_unpack_dir"
    fi

    printf " ✓\n"

    if [ "$modify_path" = "yes" ]; then
      local _path="export PATH=\"$PATH:$prefix/bin\""
      add_to_path "${HOME}/.zprofile" "${_path}"
      add_to_path "${HOME}/.profile" "${_path}"
    fi

    printf "%s Install succeeded! 🚀\n" "$_prompt"
    printf "%s To start Vector:\n" "$_prompt"
    printf "\n"
    printf "%s vector --config $prefix/config/vector.yaml\n" "$_indent"
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

  printf " ✓\n"
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
  # Required until we identify minimum supported version
  # TODO: https://github.com/vectordotdev/vector/issues/10807
  local _ldd_version
  local _glibc_version
  _ldd_version=$(ldd --version 2>&1)
  if [[ $_ldd_version =~ "GNU" ]] || [[ $_ldd_version =~ "GLIBC" ]]; then
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

check_proc() {
    # Check for /proc by looking for the /proc/self/exe link
    # This is only run on Linux
    if ! test -L /proc/self/exe ; then
        err "fatal: Unable to find /proc/self/exe.  Is /proc mounted?  Installation cannot proceed without /proc."
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
    local _ostype _cputype _bitness _arch _clibtype
    _ostype="$(uname -s)"
    _cputype="$(uname -m)"
    _clibtype="gnu"

    if [ "$_ostype" = Linux ]; then
        if [ "$(uname -o)" = Android ]; then
            _ostype=Android
        fi
        if ldd --version 2>&1 | grep -q 'musl'; then
            _clibtype="musl"
        fi
    fi

    if [ "$_ostype" = Darwin ] && [ "$_cputype" = i386 ]; then
        # Darwin `uname -m` lies
        if sysctl hw.optional.x86_64 | grep -q ': 1'; then
            _cputype=x86_64
        fi
    fi

    if [ "$_ostype" = SunOS ]; then
        # Both Solaris and illumos presently announce as "SunOS" in "uname -s"
        # so use "uname -o" to disambiguate.  We use the full path to the
        # system uname in case the user has coreutils uname first in PATH,
        # which has historically sometimes printed the wrong value here.
        if [ "$(/usr/bin/uname -o)" = illumos ]; then
            _ostype=illumos
        fi

        # illumos systems have multi-arch userlands, and "uname -m" reports the
        # machine hardware name; e.g., "i86pc" on both 32- and 64-bit x86
        # systems.  Check for the native (widest) instruction set on the
        # running kernel:
        if [ "$_cputype" = i86pc ]; then
            _cputype="$(isainfo -n)"
        fi
    fi

    case "$_ostype" in

        Android)
            _ostype=linux-android
            ;;

        Linux)
            check_proc
            _ostype=unknown-linux-$_clibtype
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

        illumos)
            _ostype=unknown-illumos
            ;;

        MINGW* | MSYS* | CYGWIN* | Windows_NT)
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

        aarch64 | arm64)
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
        riscv64)
            _cputype=riscv64gc
            ;;
        *)
            err "unknown CPU type: $_cputype"

    esac

    # Detect 64-bit linux with 32-bit userland
    if [ "${_ostype}" = unknown-linux-gnu ] && [ "${_bitness}" -eq 32 ]; then
        case $_cputype in
            x86_64)
                if [ -n "${RUSTUP_CPUTYPE:-}" ]; then
                    _cputype="$RUSTUP_CPUTYPE"
                else {
                    # 32-bit executable for amd64 = x32
                    if is_host_amd64_elf; then {
                         echo "This host is running an x32 userland; as it stands, x32 support is poor," 1>&2
                         echo "and there isn't a native toolchain -- you will have to install" 1>&2
                         echo "multiarch compatibility with i686 and/or amd64, then select one" 1>&2
                         echo "by re-running this script with the RUSTUP_CPUTYPE environment variable" 1>&2
                         echo "set to i686 or x86_64, respectively." 1>&2
                         echo 1>&2
                         echo "You will be able to add an x32 target after installation by running" 1>&2
                         echo "  rustup target add x86_64-unknown-linux-gnux32" 1>&2
                         exit 1
                    }; else
                        _cputype=i686
                    fi
                }; fi
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
            riscv64gc)
                err "riscv64 with 32-bit userland unsupported"
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

say() {
    printf 'vector: %s\n' "$1"
}

err() {
    say "$1" >&2
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
    if ! "$@"; then err "command failed: $*"; fi
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
    local _ciphersuites
    local _err
    local _status
    local _retry
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
        check_curl_for_retry_support
        _retry="$RETVAL"
        get_ciphersuites_for_curl
        _ciphersuites="$RETVAL"
        if [ -n "$_ciphersuites" ]; then
            _err=$(curl $_retry --proto '=https' --tlsv1.2 --ciphers "$_ciphersuites" --silent --show-error --fail --location "$1" --output "$2" 2>&1)
            _status=$?
        else
            echo "Warning: Not enforcing strong cipher suites for TLS, this is potentially less secure"
            if ! check_help_for "$3" curl --proto --tlsv1.2; then
                echo "Warning: Not enforcing TLS v1.2, this is potentially less secure"
                _err=$(curl $_retry --silent --show-error --fail --location "$1" --output "$2" 2>&1)
                _status=$?
            else
                _err=$(curl $_retry --proto '=https' --tlsv1.2 --silent --show-error --fail --location "$1" --output "$2" 2>&1)
                _status=$?
            fi
        fi
        if [ -n "$_err" ]; then
            echo "$_err" >&2
            if echo "$_err" | grep -q 404$; then
                err "installer for platform '$3' not found, this may be unsupported"
            fi
        fi
        return $_status
    elif [ "$_dld" = wget ]; then
        if [ "$(wget -V 2>&1|head -2|tail -1|cut -f1 -d" ")" = "BusyBox" ]; then
            echo "Warning: using the BusyBox version of wget.  Not enforcing strong cipher suites for TLS or TLS v1.2, this is potentially less secure"
            _err=$(wget "$1" -O "$2" 2>&1)
            _status=$?
        else
            get_ciphersuites_for_wget
            _ciphersuites="$RETVAL"
            if [ -n "$_ciphersuites" ]; then
                _err=$(wget --https-only --secure-protocol=TLSv1_2 --ciphers "$_ciphersuites" "$1" -O "$2" 2>&1)
                _status=$?
            else
                echo "Warning: Not enforcing strong cipher suites for TLS, this is potentially less secure"
                if ! check_help_for "$3" wget --https-only --secure-protocol; then
                    echo "Warning: Not enforcing TLS v1.2, this is potentially less secure"
                    _err=$(wget "$1" -O "$2" 2>&1)
                    _status=$?
                else
                    _err=$(wget --https-only --secure-protocol=TLSv1_2 "$1" -O "$2" 2>&1)
                    _status=$?
                fi
            fi
        fi
        if [ -n "$_err" ]; then
            echo "$_err" >&2
            if echo "$_err" | grep -q ' 404 Not Found$'; then
                err "installer for platform '$3' not found, this may be unsupported"
            fi
        fi
        return $_status
    else
        err "Unknown downloader"   # should not reach here
    fi
}

check_help_for() {
    local _arch
    local _cmd
    local _arg
    _arch="$1"
    shift
    _cmd="$1"
    shift

    local _category
    if "$_cmd" --help | grep -q 'For all options use the manual or "--help all".'; then
      _category="all"
    else
      _category=""
    fi

    case "$_arch" in

        *darwin*)
        if check_cmd sw_vers; then
            case $(sw_vers -productVersion) in
                10.*)
                    # If we're running on macOS, older than 10.13, then we always
                    # fail to find these options to force fallback
                    if [ "$(sw_vers -productVersion | cut -d. -f2)" -lt 13 ]; then
                        # Older than 10.13
                        echo "Warning: Detected macOS platform older than 10.13"
                        return 1
                    fi
                    ;;
                11.*)
                    # We assume Big Sur will be OK for now
                    ;;
                *)
                    # Unknown product version, warn and continue
                    echo "Warning: Detected unknown macOS major version: $(sw_vers -productVersion)"
                    echo "Warning TLS capabilities detection may fail"
                    ;;
            esac
        fi
        ;;

    esac

    for _arg in "$@"; do
        if ! "$_cmd" --help "$_category" | grep -q -- "$_arg"; then
            return 1
        fi
    done

    true # not strictly needed
}

# Check if curl supports the --retry flag, then pass it to the curl invocation.
check_curl_for_retry_support() {
  local _retry_supported=""
  # "unspecified" is for arch, allows for possibility old OS using macports, homebrew, etc.
  if check_help_for "notspecified" "curl" "--retry"; then
    _retry_supported="--retry 3"
  fi

  RETVAL="$_retry_supported"

}

# Return cipher suite string specified by user, otherwise return strong TLS 1.2-1.3 cipher suites
# if support by local tools is detected. Detection currently supports these curl backends:
# GnuTLS and OpenSSL (possibly also LibreSSL and BoringSSL). Return value can be empty.
get_ciphersuites_for_curl() {
    if [ -n "${RUSTUP_TLS_CIPHERSUITES-}" ]; then
        # user specified custom cipher suites, assume they know what they're doing
        RETVAL="$RUSTUP_TLS_CIPHERSUITES"
        return
    fi

    local _openssl_syntax="no"
    local _gnutls_syntax="no"
    local _backend_supported="yes"
    if curl -V | grep -q ' OpenSSL/'; then
        _openssl_syntax="yes"
    elif curl -V | grep -iq ' LibreSSL/'; then
        _openssl_syntax="yes"
    elif curl -V | grep -iq ' BoringSSL/'; then
        _openssl_syntax="yes"
    elif curl -V | grep -iq ' GnuTLS/'; then
        _gnutls_syntax="yes"
    else
        _backend_supported="no"
    fi

    local _args_supported="no"
    if [ "$_backend_supported" = "yes" ]; then
        # "unspecified" is for arch, allows for possibility old OS using macports, homebrew, etc.
        if check_help_for "notspecified" "curl" "--tlsv1.2" "--ciphers" "--proto"; then
            _args_supported="yes"
        fi
    fi

    local _cs=""
    if [ "$_args_supported" = "yes" ]; then
        if [ "$_openssl_syntax" = "yes" ]; then
            _cs=$(get_strong_ciphersuites_for "openssl")
        elif [ "$_gnutls_syntax" = "yes" ]; then
            _cs=$(get_strong_ciphersuites_for "gnutls")
        fi
    fi

    RETVAL="$_cs"
}

# Return cipher suite string specified by user, otherwise return strong TLS 1.2-1.3 cipher suites
# if support by local tools is detected. Detection currently supports these wget backends:
# GnuTLS and OpenSSL (possibly also LibreSSL and BoringSSL). Return value can be empty.
get_ciphersuites_for_wget() {
    if [ -n "${RUSTUP_TLS_CIPHERSUITES-}" ]; then
        # user specified custom cipher suites, assume they know what they're doing
        RETVAL="$RUSTUP_TLS_CIPHERSUITES"
        return
    fi

    local _cs=""
    if wget -V | grep -q '\-DHAVE_LIBSSL'; then
        # "unspecified" is for arch, allows for possibility old OS using macports, homebrew, etc.
        if check_help_for "notspecified" "wget" "TLSv1_2" "--ciphers" "--https-only" "--secure-protocol"; then
            _cs=$(get_strong_ciphersuites_for "openssl")
        fi
    elif wget -V | grep -q '\-DHAVE_LIBGNUTLS'; then
        # "unspecified" is for arch, allows for possibility old OS using macports, homebrew, etc.
        if check_help_for "notspecified" "wget" "TLSv1_2" "--ciphers" "--https-only" "--secure-protocol"; then
            _cs=$(get_strong_ciphersuites_for "gnutls")
        fi
    fi

    RETVAL="$_cs"
}

# Return strong TLS 1.2-1.3 cipher suites in OpenSSL or GnuTLS syntax. TLS 1.2
# excludes non-ECDHE and non-AEAD cipher suites. DHE is excluded due to bad
# DH params often found on servers (see RFC 7919). Sequence matches or is
# similar to Firefox 68 ESR with weak cipher suites disabled via about:config.
# $1 must be openssl or gnutls.
get_strong_ciphersuites_for() {
    if [ "$1" = "openssl" ]; then
        # OpenSSL is forgiving of unknown values, no problems with TLS 1.3 values on versions that don't support it yet.
        echo "TLS_AES_128_GCM_SHA256:TLS_CHACHA20_POLY1305_SHA256:TLS_AES_256_GCM_SHA384:ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384"
    elif [ "$1" = "gnutls" ]; then
        # GnuTLS isn't forgiving of unknown values, so this may require a GnuTLS version that supports TLS 1.3 even if wget doesn't.
        # Begin with SECURE128 (and higher) then remove/add to build cipher suites. Produces same 9 cipher suites as OpenSSL but in slightly different order.
        echo "SECURE128:-VERS-SSL3.0:-VERS-TLS1.0:-VERS-TLS1.1:-VERS-DTLS-ALL:-CIPHER-ALL:-MAC-ALL:-KX-ALL:+AEAD:+ECDHE-ECDSA:+ECDHE-RSA:+AES-128-GCM:+CHACHA20-POLY1305:+AES-256-GCM"
    fi
}

main "$@" || exit 1
