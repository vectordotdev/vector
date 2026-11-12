Update the experimental `native_json` codec to emit canonical Protobuf JSON using Vector's stable
Protobuf event model. The new representation preserves transportable event metadata and omits
deprecated Protobuf fields, while the decoder continues to accept the previous native JSON shape.

Compatibility between versions before and after this change:

| Producer → consumer | Behavior |
| --- | --- |
| Previous `native_json` → new decoder | Supported through the legacy reader. |
| New `native_json` → previous decoder | Unsupported; previous versions cannot read the new `{"event": ...}` envelope. |

For rolling upgrades of `native_json` pipelines, upgrade consumers before producers, starting at the
downstream end. Intermediaries that encode with `native_json` emit the new format after upgrading, even
when reading legacy input. For rollbacks, roll back producers first and drain queued data in the new
format before rolling back consumers. Decoding legacy input cannot restore metadata omitted by older
producers.

Use binary `native` for mixed-version Vector pipelines that need compatibility in both directions.
Configure both ends to use `native` with matching framing, such as `length_delimited` over TCP.

authors: pront
