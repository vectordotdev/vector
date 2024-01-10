# Set up our Cargo path so we can do Rust-y things.
echo "$HOME\.cargo\bin" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append

# We have to limit our Cargo build concurrency otherwise we can overwhelm the machine during things
# like running tests, where it will try and build many binaries at once, consuming all of the memory
# and making things go veryyyyyyy slow.
$N_JOBS=(((Get-CimInstance -ClassName Win32_ComputerSystem).NumberOfLogicalProcessors / 2),1 | Measure-Object -Max).Maximum
echo "CARGO_BUILD_JOBS=$N_JOBS" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append

if ($env:RELEASE_BUILDER -ne "true") {
    # Ensure we have cargo-next test installed.
    rustup run stable cargo install cargo-nextest --version 0.9.64 --locked
}

# Install some required dependencies / tools.
choco install make
choco install protoc

# Set a specific override path for libclang.
echo "LIBCLANG_PATH=$((gcm clang).source -replace "clang.exe")" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append

# Explicitly instruct the `openssl` crate to use Strawberry Perl instead of the Perl bundled with
# git-bash, since the GHA Windows 2022 image has a poorly arranged PATH.
echo "OPENSSL_SRC_PERL=C:\Strawberry\perl\bin\perl.exe" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append

# Force the proto-build crate to avoid building the vendored protoc.
echo "PROTO_NO_VENDOR=1" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
