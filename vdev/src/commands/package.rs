crate::cli_subcommands! {
    "Package Vector in various formats"
        archive, deb, msi, rpm,
}

crate::script_wrapper! {
    archive = "package-archive.sh"
        "Create a .tar.gz package for the specified $TARGET"
}
crate::script_wrapper! {
    deb = "package-deb.sh"
        "Create a .deb package to be distributed in the APT package manager"
}
crate::script_wrapper! {
    msi = "package-msi.sh"
        "Create a .msi package for Windows"
}
crate::script_wrapper! {
    rpm = "package-rpm.sh"
        "Create a .rpm package to be distributed in the YUM package manager"
}
