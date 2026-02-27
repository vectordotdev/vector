Added three new Cargo feature flags to control which categories of VRL functions are compiled into Vector:

- `vrl-functions-env`: enables VRL functions that access environment variables
- `vrl-functions-system`: enables VRL functions that access system information
- `vrl-functions-network`: enables VRL functions that perform network operations

All three flags are enabled by default via the `base` feature set. Builders can disable individual categories by excluding specific flags from their feature selection.

authors: dd-sebastien-lb
