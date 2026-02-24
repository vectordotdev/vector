{
  "remap": {
    "functions": {
      "encode_zstd": {
        "anchor": "encode_zstd",
        "name": "encode_zstd",
        "category": "Codec",
        "description": "Encodes the `value` to [Zstandard](https://facebook.github.io/zstd).",
        "arguments": [
          {
            "name": "value",
            "description": "The string to encode.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "compression_level",
            "description": "The default compression level.",
            "required": false,
            "type": [
              "integer"
            ],
            "default": "3"
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "examples": [
          {
            "title": "Encode to Zstd",
            "source": "encode_base64(encode_zstd(\"please encode me\"))",
            "return": "KLUv/QBYgQAAcGxlYXNlIGVuY29kZSBtZQ=="
          }
        ],
        "pure": true
      }
    }
  }
}
