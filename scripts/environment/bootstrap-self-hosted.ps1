Get-ChildItem -Path . -Include * -File -Recurse | foreach { $_.Delete()}
