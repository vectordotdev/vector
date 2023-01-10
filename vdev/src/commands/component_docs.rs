crate::cli_subcommands! {
    "Manage component documentation"
        generate, check,
}

crate::script_wrapper! {
    generate = "scripts/generate-component-docs.rb"
        "(Re)Generate component documentation"
}
crate::script_wrapper! {
    check = "scripts/check-component-docs.sh"
        "Check component documentation is up-to-date"
}
