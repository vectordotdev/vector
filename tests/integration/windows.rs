use std::process::Command;

#[test]
fn executable_version_information() {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Diagnostics.FileVersionInfo]::GetVersionInfo($env:VECTOR_EXE) | ConvertTo-Json -Compress",
        ])
        .env("VECTOR_EXE", env!("CARGO_BIN_EXE_vector"))
        .output()
        .expect("Failed to read Windows executable version information");
    assert!(
        output.status.success(),
        "PowerShell failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(info["FileVersion"], env!("CARGO_PKG_VERSION"));
    assert_eq!(info["ProductVersion"], env!("CARGO_PKG_VERSION"));
    assert_eq!(info["ProductName"], "Vector");
    assert_eq!(info["FileDescription"], env!("CARGO_PKG_DESCRIPTION"));
    assert_eq!(info["OriginalFilename"], "vector.exe");

    for (field, expected) in [
        ("MajorPart", env!("CARGO_PKG_VERSION_MAJOR")),
        ("MinorPart", env!("CARGO_PKG_VERSION_MINOR")),
        ("BuildPart", env!("CARGO_PKG_VERSION_PATCH")),
        ("PrivatePart", "0"),
    ] {
        let expected: u16 = expected.parse().unwrap();
        assert_eq!(info[format!("File{field}")], expected);
        assert_eq!(info[format!("Product{field}")], expected);
    }
}
