# Fail immediately on any error
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# Helper: download a file with exponential backoff retry.
function Invoke-DownloadWithRetry {
    param(
        [string]$Url,
        [string]$Destination,
        [int]$MaxRetries = 5
    )

    for ($attempt = 1; $attempt -le $MaxRetries; $attempt++) {
        try {
            Invoke-WebRequest -Uri $Url -OutFile $Destination -UseBasicParsing
            return
        } catch {
            if ($attempt -lt $MaxRetries) {
                $delay = 5 * [math]::Pow(2, $attempt)  # 10, 20, 40, 80 seconds
                Write-Host "Download of $Url failed (attempt $attempt of $MaxRetries): $($_.Exception.Message). Retrying in $delay seconds..."
                Start-Sleep -Seconds $delay
            } else {
                throw "Download of $Url failed after $MaxRetries attempts: $($_.Exception.Message)"
            }
        }
    }
}

# Set up our Cargo path so we can do Rust-y things.
echo "$HOME\.cargo\bin" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append

if ($env:RELEASE_BUILDER -ne "true") {
    bash scripts/environment/prepare.sh --modules=cargo-nextest
} else {
    bash scripts/environment/prepare.sh --modules=rustup
}

# Install protoc directly from the upstream GitHub release. This matches the
# pinned version used on Linux/macOS (see scripts/environment/install-protoc.sh)
# and avoids the recurring Chocolatey CDN failures ("Chocolatey installed 0/0
# packages") that used to block Windows CI jobs.
$ProtocVersion = "21.12"
$ProtocDir = Join-Path $env:RUNNER_TEMP "protoc"
$ProtocZip = Join-Path $env:RUNNER_TEMP "protoc.zip"
$ProtocUrl = "https://github.com/protocolbuffers/protobuf/releases/download/v${ProtocVersion}/protoc-${ProtocVersion}-win64.zip"

Write-Host "Downloading protoc v${ProtocVersion} from ${ProtocUrl}"
Invoke-DownloadWithRetry -Url $ProtocUrl -Destination $ProtocZip
Expand-Archive -Path $ProtocZip -DestinationPath $ProtocDir -Force
echo "$ProtocDir\bin" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append

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
