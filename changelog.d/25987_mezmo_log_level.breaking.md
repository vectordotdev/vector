# `mezmo` sink sends `level` as the line level {#mezmo-sink-level}

## Summary

The `mezmo` sink now sends an event's top-level string `level` field as the line's `level`, like it already does for `app`, `env` and `file`, so Mezmo shows and filters the log severity. Previously `level` was sent inside `meta`, so it no longer appears as `meta.level`. A `level` that is not a string stays in `meta`.

## Migration

Searches, views and alerts in Mezmo that match on `meta.level` must match on the line's level instead.

authors: anpawo
