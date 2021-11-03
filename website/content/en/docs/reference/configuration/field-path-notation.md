---
title: Field path notation reference
short: Field paths
weight: 7
---

Throughout Vector's configuration you'll notice that certain options take field paths as values. In order to reference nested or array values, you can use Vector's field path notation. This notation is not anything special, it simply uses `.` and `[<index>]` to access nested and array values, respectively.

## Example

Let's take this log event:

```json
{
  "timestamp": "2020-02-14T01:22:23.223Z",
  "application_id": 1,
  "message": "Hello world",
  "field.with.dot": "value",
  "ec2": {
    "instance_id": "abcd1234",
    "tags": ["tag1: value1", "tag2: value1"]
  }
}
```

We can access the values like so:

Path | Value
:----|:-----
`"application_id"` | The root-level `application_id` field
`"ec2.instance_id"` | The child `instance_id` field
`"ec2.tags[0]"` | The first value in the child `tags` array

## Syntax

### Root-level values

Root-level values can be accessed by supplying the name of the field, as shown in the example above:

```toml
field_name
```

### Nested values

Nested values can be accessed by separating ancestor fields using the `.` character:

```toml
grandparent.parent.child
```

### Array Values

Array values can be accessed using `[<index>]` syntax. This accesses the first value since it has an index of 0:

```toml
field_name[0]
```

This accesses the first value of the nested child field:

```toml
parent.child[0]
```

### Escaping

The special characters `.`, `[`, and `]` can be escaped with a `\`:

```toml
field\.with\.dots
```

The above name is treated literally.

The `\` character, if used literally, must be escaped with a `\` as well.
