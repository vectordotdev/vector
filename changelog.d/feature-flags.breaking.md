Removed the `default-no-vrl-cli` feature flag when compiling Vector. Despite the name, that
flag didn't actually disable the VRL cli. Use the `default-no-api-client` flag instead which
is equivalent but with the addition of enrichment tables.

authors: thomasqueirozb
