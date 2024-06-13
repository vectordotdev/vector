// This file establishes the Vector repo as a CUE module that can be imported by
// other CUE libraries. This is here largely so that the CUE team can use the
// the Vector docs as an integration test case. See
// https://github.com/vectordotdev/vector/pull/6593. This currently has no effect
// on the Vector docs build.

module: "vector.dev"
language: {
	version: "v0.9.0"
}
