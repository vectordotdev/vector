`vector validate` now resolves `SECRET[backend.key]` placeholders from the configured secret
backends before validating the configuration, matching `vector`'s startup behavior.
If `--no-environment` is specified then secrets aren't resolved by default. You can specify
the new `--resolve-secrets` flag to resolve secrets as well.

authors: thomasqueirozb
