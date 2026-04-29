The `vrl` codec now supports an `inject_metadata` option. When set to `true`, sources can inject per-request metadata into the VRL program before it executes, making source-specific context readable via `%`-prefixed paths (e.g. `%exec.host`, `%exec.command`, `%vector.secrets.*`). The `exec` source is the first to support this. VRL-produced metadata always takes priority over injected values on collision.

authors: thomasqueirozb
