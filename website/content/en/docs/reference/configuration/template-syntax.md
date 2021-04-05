---
title: Template syntax
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

The value of the key_prefix option would equal:

```
application_id=1/date=2020-02-14
```

Because the [`aws_s3`][aws_s3] sink batches data, each event would be grouped by its produced value. This effectively enables dynamic partitioning, something fundamental to storing log data in filesystems.

## Syntax

TODO

## How it works

### Array Values

Array values can be accessed using Vector's [field notation syntax][paths]:

```
option = "{{ parent.child[0] }}"
```

### Fallback values

Vector doesn't currently support fallback values. [Issue 1692][1692] is open to add this functionality. In the interim, you can use the [`lua` transform][lua] to set a default value:

```toml
[transforms.set_defaults]
  # REQUIRED
  type = "lua"
  inputs = ["my-source-id"]
  source = '''
    if event["my_field"] == nil then
      event["my_field"] = "default"
    end
  '''
```

### Missing fields

If a field is missing, a blank string is inserted in its place. In that case, Vector neither errors nor drops the event nor logs anything.

### Nested fields

Nested values can be accessed using Vector's [field notation syntax][paths]:

```
option = "{{ parent.child[0] }}"
```


[1692]: https://github.com/timberio/vector/issues/1692
[aws_s3]: /docs/reference/configuration/sinks/aws_s3
[paths]: /docs/reference/configuration/field-path-notation
