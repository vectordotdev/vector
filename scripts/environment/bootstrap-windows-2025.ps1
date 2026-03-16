# Fail immediately on any error
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# Helper function to install choco packages with exponential backoff retry
function Install-ChocoPackage {
    param(
        [string]$Package,
        [int]$MaxRetries = 5
    )

    for ($attempt = 1; $attempt -le $MaxRetries; $attempt++) {
        choco install $Package --execution-timeout=7200 -y
        if ($LASTEXITCODE -eq 0) {
            return
        }

        if ($attempt -lt $MaxRetries) {
            $delay = 5 * [math]::Pow(2, $attempt)  # Exponential: 10, 20, 40, 80 seconds
            Write-Host "choco install $Package failed (attempt $attempt of $MaxRetries). Retrying in $delay seconds..."
            Start-Sleep -Seconds $delay
        } else {
            throw "choco install $Package failed after $MaxRetries attempts"
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

# Install Chocolatey packages with exponential backoff retry
Install-ChocoPackage "make"
Install-ChocoPackage "protoc"

# Set a specific override path for libclang.
echo "LIBCLANG_PATH=$( (gcm clang).source -replace "clang.exe" )" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append

# Explicitly instruct the `openssl` crate to use Strawberry Perl instead of the Perl bundled with
# git-bash, since the GHA Windows 2022 image has a poorly arranged PATH.
echo "OPENSSL_SRC_PERL=C:\Strawberry\perl\bin\perl.exe" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append

# Force the proto-build crate to avoid building the vendored protoc.
echo "PROTO_NO_VENDOR=1" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
