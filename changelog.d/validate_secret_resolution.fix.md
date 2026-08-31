`vector validate` now resolves `SECRET[backend.key]` placeholders from the configured secret
backends before validating the configuration, matching `vector`'s startup behavior.
Now `vector validate` always resolves secrets unless `--no-environment` is specified.
Use the new `--resolve-secrets` flag with `--no-environment` to resolve secrets without
additional environment access.

authors: thomasqueirozb
