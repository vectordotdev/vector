#!/usr/bin/env bash
# Run only in a disposable test container with no cloud or signing credentials.
set -Eeuo pipefail
scenario=${1:?Expected early, late, or fresh}
[[ "$scenario" == early || "$scenario" == late || "$scenario" == fresh ]]
[[ -f /.dockerenv && -f /bundle.tar && -f /setup.sh ]]
work=$(mktemp -d)
server_pid=
cleanup() {
    status=$?
    if ((status)); then
        tail -n 60 "$work/command.log" "$work/refresh.log" "$work/http.log" 2>/dev/null || true
        if [[ -n "${source_file:-}" && -f "$source_file" ]]; then cat "$source_file"; fi
        if command -v apt-cache >/dev/null; then apt-cache policy vector || true; fi
    fi
    [[ -z "$server_pid" ]] || kill "$server_pid" 2>/dev/null || true
    rm -rf "$work" /run/installer-keys/*
}
trap cleanup EXIT
trap 'printf "Installer test failed at line %s\n" "$LINENO" >&2' ERR
mkdir -p "$work/bundle" "$work/http/keys" /run/installer-keys
tar -xf /bundle.tar -C "$work/bundle"
cp "$work/bundle/public/"* "$work/http/keys/"
select_view() {
    ln -s "$work/bundle/views/$1" "$work/http/next"
    mv -Tf "$work/http/next" "$work/http/current"
}
select_view legacy
openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj /CN=localhost \
    -addext 'subjectAltName=DNS:localhost,IP:127.0.0.1' \
    -keyout /run/installer-keys/tls.key -out /run/installer-keys/tls.crt > "$work/command.log" 2>&1
if command -v apt-get >/dev/null; then
    family=apt
    install -m 644 /run/installer-keys/tls.crt /usr/local/share/ca-certificates/vector-installer-test.crt
    update-ca-certificates > "$work/command.log" 2>&1
else
    family=rpm
    install -m 644 /run/installer-keys/tls.crt /etc/pki/ca-trust/source/anchors/vector-installer-test.crt
    update-ca-trust > "$work/command.log" 2>&1
fi
python3 - "$work/http" > "$work/http.log" 2>&1 <<'PY' &
import http.server, os, socketserver, ssl, sys
class Handler(http.server.SimpleHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def send_head(self):
        # The archived views were copied together and may share a filesystem
        # mtime. Always return the selected bytes, while apt retains its cached
        # Release identity for comparison. CDN caching is a separate test.
        if "If-Modified-Since" in self.headers:
            del self.headers["If-Modified-Since"]
        return super().send_head()
# AlmaLinux 8's system Python predates ThreadingHTTPServer and the directory
# parameter. Keep the fixture compatible without changing the client runtime.
class Server(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
os.chdir(sys.argv[1])
server = Server(("127.0.0.1", 18766), Handler)
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain("/run/installer-keys/tls.crt", "/run/installer-keys/tls.key")
server.socket = context.wrap_socket(server.socket, server_side=True)
server.serve_forever()
PY
server_pid=$!
for _ in $(seq 1 30); do curl -fsS https://localhost:18766/ >/dev/null 2>&1 && break; sleep .2; done
curl -fsS https://localhost:18766/ >/dev/null
export VECTOR_PUBLIC_KEY_URL=https://localhost:18766/keys/cose.asc
VECTOR_PUBLIC_KEY_FINGERPRINT=$(cat "$work/bundle/public/fingerprint")
export VECTOR_PUBLIC_KEY_FINGERPRINT
base=https://localhost:18766/current
pass() { printf 'PASS: %s %s: %s\n' "$family" "$scenario" "$*"; }
setup() { bash /setup.sh "$@"; }
expect_failure() {
    local pattern=$1; shift
    if "$@" > "$work/command.log" 2>&1; then printf 'Unexpected success: %s\n' "$*" >&2; exit 1; fi
    grep -Eiq "$pattern" "$work/command.log" || { cat "$work/command.log"; exit 1; }
}
if [[ "$family" == apt ]]; then
    source_file=/etc/apt/sources.list.d/vector.list
    shared_key=/usr/share/keyrings/datadog-archive-keyring.gpg
    owned_key=/usr/share/keyrings/vector-archive-keyring.gpg
    timber_file=/etc/apt/sources.list.d/timber-vector.list
    refresh() { apt-get -o Dir::Etc::sourcelist=sources.list.d/vector.list -o Dir::Etc::sourceparts=- -o APT::Get::List-Cleanup=0 -o APT::Update::Error-Mode=any update; }
    install_version() { DEBIAN_FRONTEND=noninteractive apt-get -y --allow-downgrades install "vector=$1-1"; }
    # Resolve normal distro dependencies separately; later refreshes retain the
    # actual cached Vector Release information across the simulated cutover.
    apt-get update > "$work/command.log" 2>&1
else
    source_file=/etc/yum.repos.d/vector.repo
    shared_key=/etc/pki/rpm-gpg/LEGACY-INSTALLER-TEST
    owned_key=/etc/pki/rpm-gpg/RPM-GPG-KEY-vector
    timber_file=/etc/yum.repos.d/timber-vector.repo
    refresh() {
        dnf -y --disablerepo='*' --enablerepo=vector clean metadata
        dnf -y --disablerepo='*' --enablerepo=vector --refresh makecache
    }
    install_version() { dnf -y install "vector-$1-1"; }
fi

if [[ "$scenario" == fresh ]]; then
    select_view upgraded
    expect_failure 'requires an existing|No .* found' setup --migrate
    expect_failure 'cannot be disabled' env VEC_RPM_REPO_GPGCHECK=0 bash /setup.sh
    expect_failure 'HTTPS URL' env VECTOR_PUBLIC_KEY_URL=http://localhost/key bash /setup.sh
    expect_failure 'pinned primary fingerprint' env VECTOR_PUBLIC_KEY_URL=https://localhost:18766/keys/legacy.asc bash /setup.sh
    [[ ! -e "$source_file" && ! -e "$owned_key" ]]
    pass 'invalid trust inputs fail before creating repository files'

    # Both legacy override spellings accept HTTPS URLs and retain strict checks.
    VECTOR_DIST_CHANNEL=beta TESTING_APT_URL="$base/apt" TESTING_YUM_URL="$base/yum" setup > "$work/command.log" 2>&1
    grep -q beta "$source_file"
    setup --migrate > "$work/command.log" 2>&1
    grep -q beta "$source_file"
    pass 'fresh beta setup and repeated migration retain the channel'
    rm "$source_file" "$source_file.pre-cose"
    TESTING_APT_REPO_VERSION='stable vector-0' TESTING_YUM_VERSION_PATH=stable/vector-0 \
        TESTING_APT_URL="$base/apt" TESTING_YUM_URL="$base/yum" setup > "$work/command.log" 2>&1
    if [[ "$family" == apt ]]; then
        [[ $(gpg --batch --with-colons --show-keys "$owned_key" | grep -c '^pub:') == 1 ]]
    else
        grep -qx 'gpgcheck=1' "$source_file"
        grep -qx 'repo_gpgcheck=1' "$source_file"
        if grep -q legacy "$source_file"; then exit 1; fi
    fi
    install_version 0.60.0 > "$work/command.log" 2>&1
    vector --version | grep -F 'vector 0.60.0 '
    getent passwd vector >/dev/null
    getent group vector >/dev/null
    pass 'fresh stable installation uses only COSE trust over verified HTTPS'
    exit 0
fi

if [[ "$family" == apt ]]; then
    install -m 644 "$work/bundle/public/legacy.gpg" "$shared_key"
    printf '# existing Vector settings\ndeb [arch=%s signed-by=%s] %s/apt/ stable vector-0\n' \
        "$(dpkg --print-architecture)" "$shared_key" "$base" > "$source_file"
else
    install -m 644 "$work/bundle/public/legacy.asc" "$shared_key"
    rpm --import "$shared_key"
    cat > "$source_file" <<EOF
# existing Vector settings
[vector]
name=Existing Vector
baseurl=$base/yum/stable/vector-0/\$basearch/
enabled=1
gpgcheck=1
repo_gpgcheck=1
priority=7
exclude=other-package
gpgkey=file://$shared_key
       https://localhost:18766/keys/legacy.asc
EOF
fi
shared_hash=$(sha256sum "$shared_key")
source_hash=$(sha256sum "$source_file")
cp "$source_file" "$work/original-source"
refresh > "$work/command.log" 2>&1
install_version 0.59.0 > "$work/command.log" 2>&1
vector --version | grep -F 'vector 0.59.0 '
mkdir -p /etc/vector
printf '# preserve installer migration configuration\n' > /etc/vector/vector.yaml
account=$(id vector)
pass 'legacy installation succeeds with cached metadata and legacy-only trust'

expect_failure 'pinned primary fingerprint' env VECTOR_PUBLIC_KEY_URL=https://localhost:18766/keys/legacy.asc bash /setup.sh --migrate
expect_failure 'fresh setup only' env VECTOR_DIST_CHANNEL=beta bash /setup.sh --migrate
if [[ "$family" == apt ]]; then
    printf 'Types: deb\nURIs: %s/apt\nSuites: stable\nComponents: vector-0\n' "$base" > /etc/apt/sources.list.d/custom-vector.sources
    expect_failure 'Additional Vector source' setup --migrate
    rm /etc/apt/sources.list.d/custom-vector.sources
else
    # shellcheck disable=SC2016
    printf '[other-vector]\nbaseurl=%s/yum/stable/vector-0/$basearch/\n' "$base" > /etc/yum.repos.d/custom-vector.repo
    expect_failure 'Additional Vector source' setup --migrate
    rm /etc/yum.repos.d/custom-vector.repo
fi
[[ "$source_hash" == "$(sha256sum "$source_file")" && ! -e "$owned_key" && ! -e "$source_file.pre-cose" ]]
pass 'wrong key, channel override and ambiguous custom sources leave existing files untouched'

if [[ "$scenario" == late ]]; then
    select_view cose
    expect_failure 'NO_PUBKEY|public key|GPG|signature' refresh
    pass 'a client that missed the announcement rejects the new key before migration'
else
    # The opt-in legacy Cloudsmith file is disabled with a recoverable backup.
    printf '# old source at https://repositories.timber.io/public/vector\n' > "$timber_file"
    export CSM_MIGRATE=true
fi
setup --migrate > "$work/command.log" 2>&1
[[ "$shared_hash" == "$(sha256sum "$shared_key")" ]]
[[ $(sha256sum "$source_file.pre-cose" | cut -d ' ' -f 1) == $(sha256sum "$work/original-source" | cut -d ' ' -f 1) ]]
first_source_hash=$(sha256sum "$source_file")
first_key_hash=$(sha256sum "$owned_key")
# Subsequent migration and metadata verification must no longer need the old
# remote key endpoint. The shared key on disk remains available to other apps.
rm "$work/http/keys/legacy.asc"
setup --migrate > "$work/command.log" 2>&1
[[ "$first_source_hash" == "$(sha256sum "$source_file")" && "$first_key_hash" == "$(sha256sum "$owned_key")" ]]
if [[ "$family" == apt ]]; then
    [[ $(gpg --batch --with-colons --show-keys "$owned_key" | grep -c '^pub:') == 2 ]]
    grep -q "arch=$(dpkg --print-architecture)" "$source_file"
else
    grep -qx "gpgkey=file://$owned_key" "$source_file"
    [[ $(gpg --batch --with-colons --show-keys "$owned_key" | grep -c '^pub:') == 2 ]]
    grep -qx priority=7 "$source_file"
    grep -qx exclude=other-package "$source_file"
    grep -qx gpgcheck=1 "$source_file"
    grep -qx repo_gpgcheck=1 "$source_file"
fi
pass 'migration is byte-idempotent, retains legacy trust and options, and leaves shared keys unchanged'
if [[ "$scenario" == early ]]; then
    [[ ! -e "$timber_file" && -f "$timber_file.pre-cose" ]]
    refresh > "$work/command.log" 2>&1
    pass 'pre-cutover migration still authenticates the legacy repository'
    select_view cose
    refresh > "$work/command.log" 2>&1
    pass 'cutover succeeds with cached apt identity and no release-information bypass'
fi
select_view upgraded
refresh > "$work/refresh.log" 2>&1
install_version 0.60.0 > "$work/command.log" 2>&1
vector --version | grep -F 'vector 0.60.0 '
grep -q 'preserve installer migration configuration' /etc/vector/vector.yaml
[[ "$account" == "$(id vector)" ]]
[[ "$shared_hash" == "$(sha256sum "$shared_key")" ]]
pass 'upgrade to 0.60.0 over verified HTTPS preserves configuration and account'

# After cutover, legacy trust can be retired from the Vector source even if the
# old key endpoint has gone away. Other packages' shared key files stay intact.
install -m 644 "$work/original-source" "$source_file"
if [[ "$family" == apt ]]; then
    sed -i "s|$shared_key|/usr/share/keyrings/vector-unavailable-test.gpg|" "$source_file"
fi
setup --migrate --cose-only > "$work/command.log" 2>&1
[[ $(gpg --batch --with-colons --show-keys "$owned_key" | grep -c '^pub:') == 1 ]]
[[ "$shared_hash" == "$(sha256sum "$shared_key")" ]]
refresh > "$work/command.log" 2>&1
pass 'post-cutover migration from original settings needs no legacy key source and preserves shared keys'
