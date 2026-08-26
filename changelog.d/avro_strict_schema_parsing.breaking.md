# Avro codec rejects shorthand complex type schemas {#avro-strict-schema-parsing}

## Summary

The `apache-avro` library has been upgraded from 0.21 to 0.22, which enforces stricter schema
parsing per the Avro specification. Field-level attributes must now be nested inside a `"type"`
object rather than specified as siblings of the `"type"` string:

- **Complex types** (`array`, `map`, `enum`, `record`, `fixed`): schemas using the shorthand
  form will **fail to parse at startup**.
- **Logical types** (`timestamp-millis`, `date`, `uuid`, etc.): schemas using the shorthand
  form will still parse, but the logical type is **silently ignored** and the field is treated
  as a plain primitive.

## Migration

Wrap any complex type definitions in a nested `"type"` object within your Avro schema
configuration.

#### Array

##### Old

```yaml
encoding:
  codec: avro
  avro:
    schema: |
      {
        "type": "record", "name": "Log",
        "fields": [
          {"name": "tags", "type": "array", "items": "string"}
        ]
      }
```

##### New

```yaml
encoding:
  codec: avro
  avro:
    schema: |
      {
        "type": "record", "name": "Log",
        "fields": [
          {"name": "tags", "type": {"type": "array", "items": "string"}}
        ]
      }
```

#### Map

##### Old

```yaml
{"name": "metadata", "type": "map", "values": "string"}
```

##### New

```yaml
{"name": "metadata", "type": {"type": "map", "values": "string"}}
```

#### Enum

##### Old

```yaml
{"name": "status", "type": "enum", "symbols": ["A", "B", "C"]}
```

##### New

```yaml
{"name": "status", "type": {"type": "enum", "name": "Status", "symbols": ["A", "B", "C"]}}
```

#### Fixed

##### Old

```yaml
{"name": "hash", "type": "fixed", "size": 16}
```

##### New

```yaml
{"name": "hash", "type": {"type": "fixed", "name": "Hash", "size": 16}}
```

#### Logical types (timestamp, date, uuid, etc.)

##### Old

```yaml
{"name": "created_at", "type": "long", "logicalType": "timestamp-millis"}
```

##### New

```yaml
{"name": "created_at", "type": {"type": "long", "logicalType": "timestamp-millis"}}
```

authors: omwbennett
