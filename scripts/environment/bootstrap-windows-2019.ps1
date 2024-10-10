# Set up our Cargo path so we can do Rust-y things.
echo "$HOME\.cargo\bin" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append

# We have to limit our Cargo build concurrency otherwise we can overwhelm the machine during things
# like running tests, where it will try and build many binaries at once, consuming all of the memory
# and making things go veryyyyyyy slow.
$N_JOBS=(((Get-CimInstance -ClassName Win32_ComputerSystem).NumberOfLogicalProcessors / 2),1 | Measure-Object -Max).Maximum
echo "CARGO_BUILD_JOBS=$N_JOBS" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append

if ($env:RELEASE_BUILDER -ne "true") {
    # Ensure we have cargo-next test installed.
    rustup run stable cargo install cargo-nextest --version 0.9.72 --locked
}

# Support for retries to avoid transient network issues such as 503 errors.
# This can be deleted after issue https://github.com/vectordotdev/vector/issues/21468 is resolved.
function Retry-Command {
  param (
    [Parameter(Mandatory=$true)]
    [scriptblock]$Command,

    [int]$Retries = 3,

    [int]$DelaySeconds = 10
  )

  for ($i = 0; $i -lt $Retries; $i++) {
    try {
      Invoke-Command -ScriptBlock $Command
      return
    }
    catch {
      Write-Host "Command failed with error: $_"
    }

    if ($i -lt ($Retries - 1)) {
      Write-Host "Retrying in $DelaySeconds seconds..."
      Start-Sleep -Seconds $DelaySeconds
      $DelaySeconds = $DelaySeconds * 2
    } else {
      Write-Host "No retries reached. Command failed."
      exit 1
    }
  }
}

$command_1 = {
  choco install make
}
Retry-Command -Command $command_1 -Retries 3 -DelaySeconds 2

$command_2 = {
  choco install protoc
}
Retry-Command -Command $command_2 -Retries 3 -DelaySeconds 2

# Set a specific override path for libclang.
echo "LIBCLANG_PATH=$((gcm clang).source -replace "clang.exe")" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append

# Explicitly instruct the `openssl` crate to use Strawberry Perl instead of the Perl bundled with
# git-bash, since the GHA Windows 2022 image has a poorly arranged PATH.
echo "OPENSSL_SRC_PERL=C:\Strawberry\perl\bin\perl.exe" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append

# Force the proto-build crate to avoid building the vendored protoc.
echo "PROTO_NO_VENDOR=1" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
