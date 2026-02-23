{
  "remap": {
    "functions": {
      "encode_base16": {
        "anchor": "encode_base16",
        "name": "encode_base16",
        "category": "Codec",
        "description": "Encodes the `value` to [Base16](https://en.wikipedia.org/wiki/Hexadecimal).",
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
        "examples": [
          {
            "title": "Encode to Base16",
            "source": "encode_base16(\"some string value\")",
            "return": "736f6d6520737472696e672076616c7565"
          }
        ],
        "pure": true
      }
    }
  }
}