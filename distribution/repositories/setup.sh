#!/usr/bin/env bash
# Configure Vector's apt/yum repository. This script does not install Vector.
set -euo pipefail

fail() { printf 'Vector repository setup: %s\n' "$*" >&2; exit 1; }
usage() {
    cat <<'EOF'
Usage: bash setup.sh [--migrate] [--cose-only]

With no existing Vector source, configure the stable repository with COSE trust.
With an existing vector.list or vector.repo, preserve its settings and signing
keys while adding COSE trust. --migrate requires an existing source.
After cutover, --cose-only removes legacy trust from Vector's own configuration.
Do not use --cose-only while the repository is still signed by Datadog.

Fresh setup options:
  VECTOR_DIST_CHANNEL       stable (default) or beta
  VECTOR_MAJOR_VERSION      0 (only supported major)
  VEC_REPO_URL              vector.dev (default repository domain)
  TESTING_APT_URL            HTTPS apt host/path or URL
  TESTING_YUM_URL            HTTPS yum host/path or URL
  TESTING_APT_REPO_VERSION   "<channel> vector-0"
  TESTING_YUM_VERSION_PATH   "<channel>/vector-0"

Both modes:
  VECTOR_PUBLIC_KEY_URL          HTTPS armored public-key URL
  VECTOR_PUBLIC_KEY_FINGERPRINT  Full primary fingerprint for an owned mirror
  TESTING_KEYS_URL                Compatibility alias: HTTPS host/path containing vector-packages.asc
  CSM_MIGRATE=true               Disable the old timber-vector source, with backup

Existing sources keep their channel and options; the standard ARMv7 repository
URL is corrected to armv7hl. Fresh setup overrides are rejected during migration.
Package and repository signatures remain required.
Other source filenames and deb822 files require manual migration.
EOF
}

require_existing=false
retain_legacy=true
while (($#)); do
    case "$1" in
        --migrate) require_existing=true ;;
        --cose-only) retain_legacy=false ;;
        --help|-h) usage; exit 0 ;;
        *) fail "Unknown option: $1. Use --help." ;;
    esac
    shift
done

[[ "$(uname -s)" == Linux ]] || fail "This script supports Linux apt/yum clients."
sudo_cmd=()
if ((EUID != 0)); then
    command -v sudo >/dev/null || fail "Run this script as root or install sudo."
    sudo_cmd=(sudo)
    "${sudo_cmd[@]}" -v
fi

if [[ -f /etc/debian_version ]] && command -v apt-get >/dev/null; then
    family=apt
    source_file=/etc/apt/sources.list.d/vector.list
    key_file=/usr/share/keyrings/vector-archive-keyring.gpg
    timber_file=/etc/apt/sources.list.d/timber-vector.list
elif command -v rpm >/dev/null && { command -v dnf >/dev/null || command -v yum >/dev/null; }; then
    family=rpm
    # DNF calls ARM hard-float "armhfp"; Vector publishes ARMv7 under "armv7hl".
    rpm_arch="\$basearch"
    case "$(rpm --eval '%{_arch}')" in
        armv7hl|armv7hnl|armv8hl|armv8hnl|armv8hcnl) rpm_arch=armv7hl ;;
    esac
    source_file=/etc/yum.repos.d/vector.repo
    key_file=/etc/pki/rpm-gpg/RPM-GPG-KEY-vector
    timber_file=/etc/yum.repos.d/timber-vector.repo
    package_manager=yum
    if command -v dnf >/dev/null; then package_manager=dnf; fi
else
    fail "No supported apt/yum package manager found."
fi

case "${CSM_MIGRATE:-false}" in true|false) ;; *) fail "CSM_MIGRATE must be true or false." ;; esac
[[ "${VEC_RPM_REPO_GPGCHECK:-1}" == 1 ]] || fail "Repository signature verification cannot be disabled."
[[ -z "${REPO_URL:-}" ]] || fail "Use VEC_REPO_URL for a custom repository domain."

# Never follow a managed-file symlink or replace a directory/device.
regular_or_absent() {
    [[ ! -L "$1" && ( ! -e "$1" || -f "$1" ) ]] || fail "Expected a regular file: $1"
}
for path in "$source_file" "$source_file.pre-cose" "$key_file" "$key_file.pre-cose"; do
    regular_or_absent "$path"
done
existing=false
if [[ -f "$source_file" ]]; then existing=true; fi
if "$require_existing" && ! "$existing"; then fail "No $source_file found; migrate custom sources manually."; fi
if "$existing"; then
    for option in VECTOR_DIST_CHANNEL VECTOR_MAJOR_VERSION VEC_REPO_URL TESTING_APT_URL TESTING_YUM_URL \
        TESTING_APT_REPO_VERSION TESTING_YUM_VERSION_PATH; do
        [[ -z "${!option:-}" ]] || fail "$option applies to fresh setup only; existing source settings are preserved."
    done
fi

# Refuse ambiguous parallel Vector definitions instead of creating conflicting
# Signed-By settings or rewriting a file belonging to another package.
shopt -s nullglob
if [[ "$family" == apt ]]; then
    candidates=(/etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources)
else
    candidates=(/etc/yum.repos.d/*.repo)
fi
for path in "${candidates[@]}"; do
    [[ -f "$path" && "$path" != "$source_file" ]] || continue
    if [[ "$path" == "$timber_file" && "${CSM_MIGRATE:-false}" == true ]]; then continue; fi
    if awk '!/^[[:space:]]*[#;]/ && /vector-[0-9]+|^\[vector\]/ { found=1 } END { exit !found }' "$path"; then
        fail "Additional Vector source found in $path; migrate it manually before running setup."
    fi
done
if [[ "${CSM_MIGRATE:-false}" == true && -e "$timber_file" ]]; then
    regular_or_absent "$timber_file"
    regular_or_absent "$timber_file.pre-cose"
    [[ ! -e "$timber_file.pre-cose" ]] || fail "A timber source backup already exists; inspect it before retrying."
    grep -q 'repositories.timber.io' "$timber_file" || fail "Unrecognized timber source; leaving it unchanged."
fi

work=$(mktemp -d)
cleanup() {
    if command -v gpgconf >/dev/null; then GNUPGHOME="$work/gnupg" gpgconf --kill all >/dev/null 2>&1 || true; fi
    rm -rf "$work"
}
trap cleanup EXIT
mkdir -m 700 "$work/gnupg"
export GNUPGHOME="$work/gnupg"

https_url() {
    local value=$1
    if [[ "$value" != *://* ]]; then value="https://$value"; fi
    [[ "$value" =~ ^https://[A-Za-z0-9][A-Za-z0-9._:/%+~-]*$ ]] || fail "Expected an HTTPS URL without credentials, query, or whitespace."
    printf '%s' "${value%/}"
}
default_key_url=https://dd-cose-releases.s3.us-east-1.amazonaws.com/vector/keys/vector-packages.asc
if [[ -n "${TESTING_KEYS_URL:-}" ]]; then default_key_url="$(https_url "$TESTING_KEYS_URL")/vector-packages.asc"; fi
key_url=$(https_url "${VECTOR_PUBLIC_KEY_URL:-$default_key_url}")
fingerprint=${VECTOR_PUBLIC_KEY_FINGERPRINT:-FBC83025C7F26E39DBCCDCD9ED518768DAA61968}
[[ "$fingerprint" =~ ^[0-9A-F]{40}$ ]] || fail "The primary fingerprint must contain exactly 40 uppercase hexadecimal characters."
old_keyrings=()
old_key_urls=()

if "$existing" && [[ "$family" == apt ]]; then
    mapfile -t active < <(awk 'NF && !/^[[:space:]]*#/ { print }' "$source_file")
    ((${#active[@]} == 1)) || fail "$source_file must contain exactly one active deb source; migrate custom files manually."
    apt_line='^[[:space:]]*deb[[:space:]]+\[([^]]+)\][[:space:]]+(https://[^[:space:]]+)[[:space:]]+(stable|beta)[[:space:]]+(vector-[0-9]+)[[:space:]]*(#.*)?$'
    [[ "${active[0]}" =~ $apt_line ]] || fail "Unrecognized apt source; expected HTTPS, stable/beta, and a Vector component with Signed-By."
    options=${BASH_REMATCH[1]}
    signed_by=
    for option in $options; do
        case "${option,,}" in
            trusted=*|allow-insecure=*|allow-weak=*|allow-downgrade-to-insecure=*)
                case "${option##*=}" in no|false|0) ;; *) fail "Remove insecure apt trust overrides before migration." ;; esac
                ;;
        esac
        if [[ "$option" == signed-by=* ]]; then
            [[ -z "$signed_by" ]] || fail "Multiple Signed-By options require manual migration."
            signed_by=${option#signed-by=}
        fi
    done
    [[ -n "$signed_by" ]] || fail "An explicit Signed-By keyring is required for migration."
    if "$retain_legacy"; then
        IFS=, read -r -a old_keyrings <<< "$signed_by"
        for path in "${old_keyrings[@]}"; do
            [[ "$path" =~ ^/[A-Za-z0-9_./-]+\.(gpg|asc)$ && -f "$path" && -r "$path" ]] ||
                fail "Cannot preserve Signed-By entry $path; migrate fingerprint selectors or unavailable keys manually."
        done
    fi
    awk -v key="$key_file" '/^[[:space:]]*deb[[:space:]]/ { sub(/signed-by=[^] \t]+/, "signed-by=" key) } { print }' \
        "$source_file" > "$work/source"
elif "$existing"; then
    if [[ $(grep -c '^[[:space:]]*\[' "$source_file") != 1 ]] ||
        ! grep -Eq '^[[:space:]]*\[vector\][[:space:]]*$' "$source_file"; then
        fail "$source_file must contain only the [vector] repository; migrate custom files manually."
    fi
    # Snapshot old trust locally; a migrated client must not depend on Datadog's
    # key endpoint remaining available after retirement.
    if "$retain_legacy"; then
      awk '
        /^[[:space:]]*gpgkey[[:space:]]*=/ { sub(/^[^=]*=[[:space:]]*/, ""); keys=1; print; next }
        keys && /^[[:space:]]+[^#;]/ && !/^[[:space:]]*[a-zA-Z_]+[[:space:]]*=/ { print; next }
        /^[[:space:]]*([#;]|$)/ { next }
        { keys=0 }
    ' "$source_file" > "$work/old-key-urls"
    while read -r line; do
        line=${line%%#*}
        for url in $line; do
            case "$url" in
                https://*) old_key_urls+=("$(https_url "$url")") ;;
                file:///*)
                    path=${url#file://}
                    [[ "$path" =~ ^/[A-Za-z0-9_./-]+$ && -f "$path" && -r "$path" ]] || fail "Cannot read the old RPM key $path."
                    old_keyrings+=("$path")
                    ;;
                *) fail "Old RPM keys must use HTTPS or a readable local file." ;;
            esac
        done
    done < "$work/old-key-urls"
      ((${#old_key_urls[@]} + ${#old_keyrings[@]} > 0)) || fail "No existing RPM gpgkey found; preserve legacy trust manually."
    fi
    # DNF maintains a separate metadata keyring, so configure gpgkey as well as
    # importing the combined public keys into the RPM database.
    awk -v key="file://$key_file" -v arch="$rpm_arch" '
        BEGIN { keys=0; seen=0 }
        /^[[:space:]]*gpgkey[[:space:]]*=/ {
            if (seen++) exit 2
            print "gpgkey=" key
            keys=1
            next
        }
        keys && /^[[:space:]]+[^#;]/ && !/^[[:space:]]*[a-zA-Z_]+[[:space:]]*=/ { next }
        /^[[:space:]]*([#;]|$)/ { print; next }
        { keys=0 }
        /^[[:space:]]*(gpgcheck|repo_gpgcheck)[[:space:]]*=/ { next }
        /^[[:space:]]*baseurl[[:space:]]*=/ && arch=="armv7hl" {
            gsub(/\/vector-0\/\$(basearch|\{basearch\})\//, "/vector-0/armv7hl/")
        }
        { print }
        END {
            if (!seen) print "gpgkey=" key
            print "gpgcheck=1"
            print "repo_gpgcheck=1"
        }' "$source_file" > "$work/source" || fail "Duplicate gpgkey settings require manual migration."
else
    channel=${VECTOR_DIST_CHANNEL:-stable}
    major=${VECTOR_MAJOR_VERSION:-0}
    [[ "$channel" == stable || "$channel" == beta ]] || fail "VECTOR_DIST_CHANNEL must be stable or beta."
    [[ "$major" == 0 ]] || fail "VECTOR_MAJOR_VERSION must be 0; other repository majors are not published."
    domain=${VEC_REPO_URL:-vector.dev}
    [[ "$domain" =~ ^[A-Za-z0-9][A-Za-z0-9.-]*$ ]] || fail "VEC_REPO_URL must be a domain name."
    if [[ "$family" == apt ]]; then
        url=$(https_url "${TESTING_APT_URL:-apt.$domain}")
        selection=${TESTING_APT_REPO_VERSION:-$channel vector-$major}
        [[ "$selection" =~ ^(stable|beta)\ vector-0$ ]] || fail "Expected stable/beta and vector-0 in the apt override."
        printf 'deb [signed-by=%s] %s/ %s\n' "$key_file" "$url" "$selection" > "$work/source"
    else
        url=$(https_url "${TESTING_YUM_URL:-yum.$domain}")
        selection=${TESTING_YUM_VERSION_PATH:-$channel/vector-$major}
        [[ "$selection" =~ ^(stable|beta)/vector-0$ ]] || fail "Expected stable/beta and vector-0 in the yum override."
        cat > "$work/source" <<EOF
[vector]
name=Vector
baseurl=$url/$selection/$rpm_arch/
enabled=1
gpgcheck=1
repo_gpgcheck=1
priority=1
gpgkey=file://$key_file
EOF
    fi
fi

if ! command -v curl >/dev/null || ! command -v gpg >/dev/null || [[ ! -s /etc/ssl/certs/ca-certificates.crt && ! -s /etc/pki/tls/certs/ca-bundle.crt ]]; then
    if [[ "$family" == apt ]]; then
        # A stale Vector key must not prevent repairing its trust. Distro package
        # installation still performs its normal authentication checks.
        "${sudo_cmd[@]}" apt-get update || printf 'Some indexes could not refresh; attempting prerequisite installation.\n' >&2
        "${sudo_cmd[@]}" env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends ca-certificates curl gnupg
    else
        "${sudo_cmd[@]}" "$package_manager" -y --disablerepo=vector install ca-certificates curl gnupg2
    fi
fi

curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
    --connect-timeout 15 --max-time 120 --retry 3 "$key_url" -o "$work/key.asc"
gpg --batch --quiet --import "$work/key.asc"
mapfile -t imported < <(gpg --batch --with-colons --list-keys | awk '$1 ~ /^pub:/ { pub=1; next } pub && /^fpr:/ { split($0,f,":"); print f[10]; pub=0 }')
[[ ${#imported[@]} == 1 && "${imported[0]}" == "$fingerprint" ]] || fail "Downloaded public key does not match the pinned primary fingerprint."
gpg --batch --with-colons --list-keys "$fingerprint" > "$work/key.info"
awk -F: -v now="$(date +%s)" '$1=="pub" { if ($2=="r" || $2=="e" || ($7!="" && $7!=0 && $7<=now)) exit 1 }' \
    "$work/key.info" || fail "The pinned public key is expired or revoked."
gpg --batch --armor --export "$fingerprint" > "$work/cose.asc"

for path in "${old_keyrings[@]}"; do gpg --batch --quiet --import "$path"; done
for url in "${old_key_urls[@]}"; do
    curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
        --connect-timeout 15 --max-time 120 --retry 3 "$url" -o "$work/old-key.asc"
    gpg --batch --quiet --import "$work/old-key.asc"
done
if [[ "$family" == apt ]]; then
    gpg --batch --export > "$work/key"
else
    gpg --batch --armor --export > "$work/key"
fi
[[ -s "$work/key" ]] || fail "Empty public keyring."

backup() {
    if [[ -f "$1" && ! -e "$1.pre-cose" ]]; then "${sudo_cmd[@]}" cp -p "$1" "$1.pre-cose"; fi
}
atomic_install() {
    local source=$1 destination=$2 temporary
    "${sudo_cmd[@]}" install -d -m 755 "$(dirname "$destination")"
    temporary=$("${sudo_cmd[@]}" mktemp "$(dirname "$destination")/.vector-setup.XXXXXX")
    if ! "${sudo_cmd[@]}" install -o root -g root -m 644 "$source" "$temporary" ||
        ! "${sudo_cmd[@]}" mv -fT "$temporary" "$destination"; then
        "${sudo_cmd[@]}" rm -f "$temporary"
        fail "Could not install $destination."
    fi
}
backup "$source_file"
backup "$key_file"
atomic_install "$work/key" "$key_file"
atomic_install "$work/source" "$source_file"
if [[ "${CSM_MIGRATE:-false}" == true && -f "$timber_file" ]]; then
    "${sudo_cmd[@]}" mv "$timber_file" "$timber_file.pre-cose"
fi

printf 'Vector trust and repository configuration written. Refreshing only the Vector repository.\n'
if [[ "$family" == apt ]]; then
    "${sudo_cmd[@]}" apt-get update -o Dir::Etc::sourcelist=sources.list.d/vector.list \
        -o Dir::Etc::sourceparts=- -o APT::Get::List-Cleanup=0 -o APT::Update::Error-Mode=any ||
        fail "Configuration was saved, but the repository refresh failed. Check the error above and rerun."
else
    "${sudo_cmd[@]}" rpm --import "$key_file"
    "${sudo_cmd[@]}" "$package_manager" -y --disablerepo='*' --enablerepo=vector clean metadata
    "${sudo_cmd[@]}" "$package_manager" -y --disablerepo='*' --enablerepo=vector makecache ||
        fail "Configuration was saved, but the repository refresh failed. Check the error above and rerun."
fi
printf 'Vector repository setup complete. Install or upgrade Vector separately with your package manager.\n'
