{
  "remap": {
    "functions": {
      "encode_lz4": {
        "anchor": "encode_lz4",
        "name": "encode_lz4",
        "category": "Codec",
        "description": "Encodes the `value` to [Lz4](https://lz4.github.io/lz4/). This function compresses the\ninput string into an lz4 block. If `prepend_size` is set to `true`, it prepends the\noriginal uncompressed size to the compressed data. This is useful for some\nimplementations of lz4 that require the original size to be known before decoding.",
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
            "name": "prepend_size",
            "description": "Whether to prepend the original size to the compressed data.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "examples": [
          {
            "title": "Encode to Lz4",
            "source": "encode_base64(encode_lz4!(\"The quick brown fox jumps over 13 lazy dogs.\", true))",
            "return": "LAAAAPAdVGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVyIDEzIGxhenkgZG9ncy4="
          }
        ],
        "pure": true
      }
    }
  }
}