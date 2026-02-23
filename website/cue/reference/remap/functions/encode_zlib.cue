{
  "remap": {
    "functions": {
      "encode_zlib": {
        "anchor": "encode_zlib",
        "name": "encode_zlib",
        "category": "Codec",
        "description": "Encodes the `value` to [Zlib](https://www.zlib.net).",
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
            "title": "Encode to Zlib",
            "source": "encode_base64(encode_zlib(\"please encode me\"))",
            "return": "eJwryElNLE5VSM1Lzk9JVchNBQA0RQX7"
          }
        ],
        "pure": true
      }
    }
  }
}