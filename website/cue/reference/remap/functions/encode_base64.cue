{
  "remap": {
    "functions": {
      "encode_base64": {
        "anchor": "encode_base64",
        "name": "encode_base64",
        "category": "Codec",
        "description": "Encodes the `value` to [Base64](https://en.wikipedia.org/wiki/Base64).",
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
            "name": "padding",
            "description": "Whether the Base64 output is [padded](https://en.wikipedia.org/wiki/Base64#Output_padding).",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          },
          {
            "name": "charset",
            "description": "The character set to use when encoding the data.",
            "required": false,
            "type": [
              "string"
            ],
            "enum": {
              "url_safe": "Modified Base64 for [URL variants](https://en.wikipedia.org/wiki/Base64#URL_applications).",
              "standard": "[Standard](https://tools.ietf.org/html/rfc4648#section-4) Base64 format."
            },
            "default": "standard"
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "examples": [
          {
            "title": "Encode to Base64 (default)",
            "source": "encode_base64(\"please encode me\")",
            "return": "cGxlYXNlIGVuY29kZSBtZQ=="
          },
          {
            "title": "Encode to Base64 (without padding)",
            "source": "encode_base64(\"please encode me, no padding though\", padding: false)",
            "return": "cGxlYXNlIGVuY29kZSBtZSwgbm8gcGFkZGluZyB0aG91Z2g"
          },
          {
            "title": "Encode to Base64 (URL safe)",
            "source": "encode_base64(\"please encode me, but safe for URLs\", charset: \"url_safe\")",
            "return": "cGxlYXNlIGVuY29kZSBtZSwgYnV0IHNhZmUgZm9yIFVSTHM="
          },
          {
            "title": "Encode to Base64 (without padding and URL safe)",
            "source": "encode_base64(\"some string value\", padding: false, charset: \"url_safe\")",
            "return": "c29tZSBzdHJpbmcgdmFsdWU"
          }
        ],
        "pure": true
      }
    }
  }
}