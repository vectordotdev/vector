---
title: Template syntax
weight: 7
aliases: ["/docs/reference/templates", "/docs/reference/configuration/templates"]
---

Vector supports a template syntax for some configuration options. This allows for dynamic values derived from event
data. Any option that supports this syntax will be clearly documented as such in the description.

## Example

Let's partition data on AWS S3 by "application_id" and "date". We can accomplish this with the `key_prefix` option in
the `aws_s3` sink:

```toml
[sinks.backup]
  type = "aws_s3"
  bucket = "all_application_logs"
  key_prefix = "application_id={{ application_id }}/date=%F/"
```

Notice that Vector allows direct field references as well as "strftime" specifiers. If we were to run the following log
event through Vector:

```json
{
  "timestamp": "2020-02-14T01:22:23.223Z",
  "application_id": 1,
  "message": "Hello world"
}
```

The value of the above `key_prefix` option would equal:

```raw
application_id=1/date=2020-02-14
```

Because the [`aws_s3`][aws_s3] sink batches data, each event would be grouped by its produced value. This effectively
enables dynamic partitioning, something fundamental to storing log data in filesystems.

## Syntax

### Event fields

Individual [log event][log] fields can be accessed using `{{ ... }}` to wrap a VRL [path expression][path_expression]:

```toml
option = "{{ .parent.child }}"
```

### Strftime specifiers

In addition to directly accessing fields, Vector offers a shortcut for injecting [strftime specifiers][strftime]:

```toml
option = "year=%Y/month=%m/day=%d/"
```

{{< info >}}
The value is derived from the [`timestamp` field](/docs/about/under-the-hood/architecture/data-model/log/#timestamps)
and the name of this field can be changed via the [global `timestamp_key` option](/docs/reference/configuration/global-options/#log_schema.timestamp_key).
{{< /info >}}

### Escaping

You can escape this syntax by prefixing the character with a `\`. For example, you can escape the event field syntax
like this:

```toml
option = "\{{ field_name }}"
```

And [strftime] specified like so:

```toml
option = "year=\%Y/month=\%m/day=\%d/"
```

Each of the values above would be treated literally.

## How it works

### Accessing fields

You can find additional examples for accessing fields in the
[path expression reference][path_expression_examples] documentation.

### Fallback values

Vector doesn't currently support fallback values, [issue 1692][1692] is open to add this functionality. In the interim,
you can use the [`remap` transform][remap] to set a default value:

```toml
[transforms.set_defaults]
  type = "remap"
  inputs = ["my-source-id"]
  source = '''
    if !exists(.my_field) {
      .my_field = "default"
    }
  '''
```

### Missing fields

If a field is missing, an error is logged and Vector drops the event. The `component_errors_total` internal
metric is incremented with an `error_type` tag of `template_failed`.

[1692]: https://github.com/vectordotdev/vector/issues/1692
[aws_s3]: /docs/reference/configuration/sinks/aws_s3
[log]: /docs/about/under-the-hood/architecture/data-model/log
[path_expression]: /docs/reference/vrl/expressions/#path
[path_expression_examples]: /docs/reference/vrl/expressions/#path-examples
[remap]: /docs/reference/configuration/transforms/remap
[strftime]: https://docs.rs/chrono/0.4.19/chrono/format/strftime/index.html#specifiers
