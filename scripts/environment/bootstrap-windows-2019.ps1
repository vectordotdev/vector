if ($env:CI -ne $null) {
    echo "$HOME/.local/bin" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
} else {
    $env:Path += ";$HOME\.cargo\bin"
}