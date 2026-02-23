{
  "remap": {
    "functions": {
      "encode_snappy": {
        "anchor": "encode_snappy",
        "name": "encode_snappy",
        "category": "Codec",
        "description": "Encodes the `value` to Snappy.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to encode.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "internal_failure_reasons": [
          "`value` cannot be encoded into a Snappy string."
        ],
        "examples": [
          {
            "title": "Encode to Snappy",
            "source": "encode_base64(encode_snappy!(\"The quick brown fox jumps over 13 lazy dogs.\"))",
            "return": "LKxUaGUgcXVpY2sgYnJvd24gZm94IGp1bXBzIG92ZXIgMTMgbGF6eSBkb2dzLg=="
          }
        ],
        "pure": true
      }
    }
  }
}