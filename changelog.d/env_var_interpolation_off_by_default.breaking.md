Environment variable interpolation in configuration files is now disabled by default. Previously, Vector interpolated `${VAR}` references in config files automatically. To restore the previous behavior, pass `--dangerously-allow-env-var-interpolation` (or set `VECTOR_DANGEROUSLY_ALLOW_ENV_VAR_INTERPOLATION=true`). The `--disable-env-var-interpolation` flag and `VECTOR_DISABLE_ENV_VAR_INTERPOLATION` environment variable have been removed.

authors: thomasqueirozb
