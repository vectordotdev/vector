# Remove strftime specifiers from the templating syntax {#remove-strftime-from-templates}

## Summary

Vector's template syntax no longer supports [strftime][strftime] specifiers. Previously a template
like `key_prefix = "application_id={{ application_id }}/date=%F/"` would render `%F` from the event
timestamp; now `%F` is treated as a literal character. The `with_tz_offset` template method and the
`timezone` option on the `file` sink (which only existed to render strftime specifiers in templates)
have been removed.

Sinks that render a timestamp from a dedicated time format option (e.g. `filename_time_format` on the
`aws_s3`, `azure_blob`, and `gcp_cloud_storage` sinks) are unaffected and still support strftime.

[strftime]: https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers

## Migration

Format timestamps into a field with the [`remap`][remap] transform, then reference that field in the
template:

#### Old

```yaml
sinks:
  backup:
    type: "aws_s3"
    key_prefix: "application_id={{ application_id }}/date=%F/"
```

#### New

```yaml
transforms:
  add_date:
    type: "remap"
    inputs:
      - "my-source-id"
    source: |
      .date = format_timestamp!(.timestamp, format: "%Y-%m-%d")

sinks:
  backup:
    type: "aws_s3"
    key_prefix: "application_id={{ application_id }}/date={{ date }}"
```

Sink defaults that previously used strftime specifiers in templates have been updated: the `key_prefix`
of the `aws_s3` and `gcp_cloud_storage` sinks and the `blob_prefix` of the `azure_blob` sink now default
to an empty string, and the `index` of the `elasticsearch` sink now defaults to `vector`.

[remap]: https://vector.dev/docs/reference/configuration/transforms/remap

authors: thomasqueirozb
