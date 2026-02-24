{
  "remap": {
    "functions": {
      "encode_gzip": {
        "anchor": "encode_gzip",
        "name": "encode_gzip",
        "category": "Codec",
        "description": "Encodes the `value` to [Gzip](https://www.gzip.org/).",
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
            "default": "6"
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "examples": [
          {
            "title": "Encode to Gzip",
            "source": "encode_base64(encode_gzip(\"please encode me\"))",
            "return": "H4sIAAAAAAAA/yvISU0sTlVIzUvOT0lVyE0FAI4R4vcQAAAA"
          }
        ],
        "pure": true
      }
    }
  }
}
