The env var (and secrets) resolution now happens after the config string is parsed into a TOML table.
As a side effect, this fixes a bug where comment lines referring to env vars (or secrets) that don't exist caused a config build error.

authors: pront
