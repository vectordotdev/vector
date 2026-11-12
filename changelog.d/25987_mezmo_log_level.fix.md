The `mezmo` sink now sends an event's top-level string `level` field as the line's `level`, like it already does for `app`, `env` and `file`, so Mezmo shows and filters the log severity. Previously `level` was sent inside `meta`, so it no longer appears as `meta.level`. A `level` that is not a string stays in `meta`.

authors: anpawo
