Now the GELF codec with stream-based sources uses null byte (`\0`) by default as messages delimiter instead of newline (`\n`) character. This better matches GELF server behavior.

### Configuration changes

In order to maintain the previous behavior, you must set the `framing.method` option to the `character_delimited` method and the `framing.character_delimited.delimiter` option to `\n` when using GELF codec with stream-based sources.

### Example configuration change for socket source

#### Previous

```yaml
sources:
  my_source_id:
    type: "socket"
    address: "0.0.0.0:9000"
    mode: "tcp"
    decoding:
      codec: "gelf"
```

#### Current

```yaml
sources:
  my_source_id:
    type: "socket"
    address: "0.0.0.0:9000"
    mode: "tcp"
    decoding:
      codec: "gelf"
    framing:
      method: "character_delimited"
    character_delimited:
      delimiter: "\n"
```

authors: jorgehermo9
