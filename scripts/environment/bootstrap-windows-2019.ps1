if ($env:CI -ne $null) {
    echo "$HOME\.cargo\bin" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
    $N_JOBS=(((Get-CimInstance -ClassName Win32_ComputerSystem).NumberOfLogicalProcessors / 2),1 | Measure-Object -Max).Maximum
    echo "CARGO_BUILD_JOBS=$N_JOBS" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
} else {
    $env:Path += ";$HOME\.cargo\bin"
}
