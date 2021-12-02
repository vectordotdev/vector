if ($env:CI -ne $null) {
    echo "$HOME\.cargo\bin" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
    echo "CARGO_BUILD_JOBS=1" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
} else {
    $env:Path += ";$HOME\.cargo\bin"
}
