`vector validate` now resolves `SECRET[backend.key]` placeholders from the configured secret
backends before validating the configuration, matching `vector`'s startup behavior.
`vector validate --no-environment` still skips secret resolution by default; pass the new
`--resolve-secrets` flag with `--no-environment` to resolve placeholders while keeping component
and health checks disabled. Without `--no-environment`, secrets are always resolved.

authors: thomasqueirozb
