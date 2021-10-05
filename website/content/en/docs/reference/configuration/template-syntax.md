---
title: Template syntax
weight: 6
aliases: ["/docs/reference/templates", "/docs/reference/configuration/templates"]
---

Vector supports a template syntax for some configuration options. This allows for dynamic values derived from event data. Options that support this syntax will be clearly documented as such in the option description.

## Example

For example, let's partition data on AWS S3 by application_id and date. We can accomplish this with the key_prefix option in the aws_s3 sink:

```toml
[sinks.backup]
  type = "aws_s3"
  bucket = "all_application_logs"
  key_prefix = "application_id={{ application_id }}/date=%F/"
```

Notice that Vector allows direct field references as well as strftime specifiers. If we were to run the following log event though Vector:

```json
{
  "timestamp": "2020-02-14T01:22:23.223Z",
  "application_id": 1,
  "message": "Hello world"
}
```

The value of the `key_prefix` option would equal:

```raw
application_id=1/date=2020-02-14
```

Because the [`aws_s3`][aws_s3] sink batches data, each event would be grouped by its produced value. This effectively enables dynamic partitioning, something fundamental to storing log data in filesystems.

## Syntax

### Event fields

Individual [log event][log] fields can be accessed using `{{ <field-path-notation> }}` syntax:

```toml
option = "{{ field_path_notation }}"
```

Vector's [field notation][fields] uses `.` to target nested fields and `[<index>]` to target array values.

### strftime specifiers

In addition to directly accessing fields, Vector offers a shortcut for injecting [strftime specifiers][strftime]:

```toml
options = "year=%Y/month=%m/day=%d/"
```

The value is derived from the [`timestamp` field][timestamp] and the name of this field can be changed via the [global `timestamp_key` option][timestamp_key].

### Escaping

You can escape this syntax by prefixing the character with a `\`. For example, you can escape the event field syntax like this:

```toml
option = "\{{ field_name }}"
```

And [strftime] specified like so:

```toml
options = "year=\%Y/month=\%m/day=\%d/"
```

Each of the values above would be treated literally.

## How it works

### Array Values

Array values can be accessed using Vector's [field notation syntax][paths]:

```toml
option = "{{ parent.child[0] }}"
```

### Fallback values

Vector doesn't currently support fallback values. [Issue 1692][1692] is open to add this functionality. In the interim, you can use the [`remap` transform][remap] to set a default value:

```toml
[transforms.set_defaults]
  # REQUIRED
  type = "remap"
  inputs = ["my-source-id"]
  source = '''
    if !exists(.my_field) {
      .my_field = "default"
    }
  '''
```

### Missing fields

If a field is missing, a blank string is inserted in its place. In that case, Vector neither errors nor drops the event nor logs anything.

### Nested fields

Nested values can be accessed using Vector's [field notation syntax][paths]:

```toml
option = "{{ parent.child[0] }}"
```


[1692]: https://github.com/vectordotdev/vector/issues/1692
[aws_s3]: /docs/reference/configuration/sinks/aws_s3
[fields]: /docs/reference/configuration/field-path-notation
[log]: /docs/about/under-the-hood/architecture/data-model/log
[paths]: /docs/reference/configuration/field-path-notation
[remap]: /docs/reference/configuration/transforms/remap
[strftime]: https://docs.rs/chrono/0.4.19/chrono/format/strftime/index.html#specifiers
[timestamp]: /docs/about/under-the-hood/architecture/data-model/log/#timestamps
[timestamp_key]: /docs/reference/configuration/global-options/#log_schema.timestamp_key
