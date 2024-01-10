crate::cli_subcommands! {
    "Package Vector in various formats..."
        archive, deb, msi, rpm,
}

crate::script_wrapper! {
    archive = "Create a .tar.gz package for the specified $TARGET"
        => "package-archive.sh"
}
crate::script_wrapper! {
    deb = "Create a .deb package to be distributed in the APT package manager"
        => "package-deb.sh"
}
crate::script_wrapper! {
    msi = "Create a .msi package for Windows"
        => "package-msi.sh"
}
crate::script_wrapper! {
    rpm = "Create a .rpm package to be distributed in the YUM package manager"
        => "package-rpm.sh"
}
