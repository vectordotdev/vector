# Set up our Cargo path so we can do Rust-y things.
echo "$HOME\.cargo\bin" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append

# We have to limit our Cargo build concurrency otherwise we can overwhelm the machine during things
# like running tests, where it will try and build many binaries at once, consuming all of the memory
# and making things go veryyyyyyy slow.
$N_JOBS=(((Get-CimInstance -ClassName Win32_ComputerSystem).NumberOfLogicalProcessors / 2),1 | Measure-Object -Max).Maximum
echo "CARGO_BUILD_JOBS=$N_JOBS" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append

# Ensure we have cargo-next test installed.
rustup run stable cargo install cargo-nextest --version 0.9.8

# Install some required dependencies / tools.
choco install make

# Update our path so that Strawberry Perl gets used for the build. This is annoying because it's
# already in the path thanks to the Github Actions image for Windows 2022, but it's after Git which
# supplies its own version of Perl on Windows, so we have to be super explicit here.
echo "C:\Strawberry\c\bin" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
echo "C:\Strawberry\perl\site\bin" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
echo "C:\Strawberry\perl\bin" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
