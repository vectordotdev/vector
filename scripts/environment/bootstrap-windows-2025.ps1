# Fail immediately on any error
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# Set up our Cargo path so we can do Rust-y things.
echo "$HOME\.cargo\bin" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append

if ($env:RELEASE_BUILDER -ne "true") {
    bash scripts/environment/prepare.sh --modules=cargo-nextest
} else {
    bash scripts/environment/prepare.sh --modules=rustup
}

# Install protoc via the shared cross-platform script. It pins the same version
# used on Linux/macOS and downloads directly from the upstream GitHub release,
# so we avoid the recurring Chocolatey CDN failures.
$ProtocInstallDir = Join-Path $env:RUNNER_TEMP "protoc-bin"
bash scripts/environment/install-protoc.sh "$ProtocInstallDir"
if ($LASTEXITCODE -ne 0) {
    throw "install-protoc.sh failed with exit code $LASTEXITCODE"
}
echo "$ProtocInstallDir" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append

# GNU make is already on PATH on the windows-2025 runner image via the
# pre-installed MinGW toolchain at C:\mingw64\bin, so no extra install is
# needed here.

# Set a specific override path for libclang.
echo "LIBCLANG_PATH=$( (gcm clang).source -replace "clang.exe" )" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append

# Explicitly instruct the `openssl` crate to use Strawberry Perl instead of the Perl bundled with
# git-bash, since the GHA Windows 2022 image has a poorly arranged PATH.
echo "OPENSSL_SRC_PERL=C:\Strawberry\perl\bin\perl.exe" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append

# Force the proto-build crate to avoid building the vendored protoc.
echo "PROTO_NO_VENDOR=1" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
