# Set up our Cargo path so we can do Rust-y things.
if ($env:CI -ne $null) {
    echo "$HOME\.cargo\bin" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
} else {
    $env:Path += ";$HOME\.cargo\bin"
}

# If we're in CI, we have to limit our Cargo build concurrency otherwise we can overwhelm the
# machine during things like running tests, where it will try and build many binaries at once,
# consuming all of the memory and making things go veryyyyyyy slow.
if ($env:CI -ne $null) {
    $N_JOBS=(((Get-CimInstance -ClassName Win32_ComputerSystem).NumberOfLogicalProcessors / 2),1 | Measure-Object -Max).Maximum
    echo "CARGO_BUILD_JOBS=$N_JOBS" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
}

# Ensure we have cargo-next test installed.
rustup run stable cargo install cargo-nextest --version 0.9.8

# Install some required dependencies / tools.
choco install make strawberryperl

# Update our path so that Strawberry Perl gets used for the build.
echo "C:\Strawberry\perl\bin" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
