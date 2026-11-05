Update the experimental `native_json` codec to emit canonical Protobuf JSON using Vector's stable
Protobuf event model. The new representation preserves transportable event metadata and omits
deprecated Protobuf fields, while the decoder continues to accept the previous native JSON shape.

Compatibility between versions before and after this change:

| Producer → consumer | Behavior |
| --- | --- |
| Previous `native_json` → new decoder | Supported through the legacy reader. |
| New `native_json` → previous decoder | Unsupported; previous versions cannot read the new `{"event": ...}` envelope. |

Use binary `native` for mixed-version Vector pipelines that need compatibility in both directions.

authors: pront
