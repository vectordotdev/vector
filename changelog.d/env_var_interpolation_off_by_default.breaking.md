Environment variable interpolation in configuration files is now disabled by default. Previously, Vector interpolated `${VAR}` references in config files automatically. To restore the previous behavior, pass `--dangerously-allow-env-var-interpolation` (or set `VECTOR_DANGEROUSLY_ALLOW_ENV_VAR_INTERPOLATION=true`). This replaces the previous `--disable-env-var-interpolation` flag, which is now a no-op.

authors: thomasqueirozb
