Removed the misleadingly-named `default-no-vrl-cli` feature flag, which did not control VRL CLI compilation.
This flag was equivalent to `default` without `api-client` and `enrichment-tables`.
Use `default-no-api-client` as a replacement (note: this includes `enrichment-tables`) or define custom features as needed.

authors: thomasqueirozb
