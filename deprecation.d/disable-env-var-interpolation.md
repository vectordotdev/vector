---
what: "`--disable-env-var-interpolation` flag and `VECTOR_DISABLE_ENV_VAR_INTERPOLATION` environment variable"
deprecated_since: "0.57.0"
---

Environment variable interpolation is now disabled by default, making `--disable-env-var-interpolation` a no-op.

Use `--dangerously-allow-env-var-interpolation` (or `VECTOR_DANGEROUSLY_ALLOW_ENV_VAR_INTERPOLATION=true`) to explicitly opt in to interpolation.
