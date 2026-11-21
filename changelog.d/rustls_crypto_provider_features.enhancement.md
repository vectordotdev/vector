Vector's Rustls crypto provider is now selected at build time with the `tls-ring` and `tls-aws-lc` Cargo features, so Vector can be built without Ring. Release builds enable `tls-ring` and keep using Ring. Custom builds that use `--no-default-features` should add `tls-ring` to keep the previous provider. If neither feature is enabled, Vector installs no provider, and an application embedding Vector must install one before starting it.

authors: apatruddog
