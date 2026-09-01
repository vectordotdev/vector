Update the experimental `native_json` codec to emit canonical Protobuf JSON using Vector's stable
Protobuf event model. The new representation preserves transportable event metadata and omits
deprecated Protobuf fields, while the decoder continues to accept the previous native JSON shape.
Native Protobuf encoding also stops writing the deprecated `Log.fields` representation while
continuing to decode payloads that contain it.

authors: pront
