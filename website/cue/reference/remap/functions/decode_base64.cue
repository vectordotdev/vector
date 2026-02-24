{
  "remap": {
    "functions": {
      "decode_base64": {
        "anchor": "decode_base64",
        "name": "decode_base64",
        "category": "Codec",
        "description": "Decodes the `value` (a [Base64](https://en.wikipedia.org/wiki/Base64) string) into its original string.",
        "arguments": [
          {
            "name": "value",
            "description": "The [Base64](https://en.wikipedia.org/wiki/Base64) data to decode.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "charset",
            "description": "The character set to use when decoding the data.",
            "required": false,
            "type": [
              "string"
            ],
            "enum": {
              "standard": "[Standard](https://tools.ietf.org/html/rfc4648#section-4) Base64 format.",
              "url_safe": "Modified Base64 for [URL variants](https://en.wikipedia.org/wiki/Base64#URL_applications)."
            },
            "default": "standard"
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "internal_failure_reasons": [
          "`value` isn't a valid encoded Base64 string."
        ],
        "examples": [
          {
            "title": "Decode Base64 data (default)",
            "source": "decode_base64!(\"eW91IGhhdmUgc3VjY2Vzc2Z1bGx5IGRlY29kZWQgbWU=\")",
            "return": "you have successfully decoded me"
          },
          {
            "title": "Decode Base64 data (URL safe)",
            "source": "decode_base64!(\"eW91IGNhbid0IG1ha2UgeW91ciBoZWFydCBmZWVsIHNvbWV0aGluZyBpdCB3b24ndA==\", charset: \"url_safe\")",
            "return": "you can't make your heart feel something it won't"
          }
        ],
        "pure": true
      }
    }
  }
}
