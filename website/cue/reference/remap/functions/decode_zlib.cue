{
  "remap": {
    "functions": {
      "decode_zlib": {
        "anchor": "decode_zlib",
        "name": "decode_zlib",
        "category": "Codec",
        "description": "Decodes the `value` (a [Zlib](https://www.zlib.net) string) into its original string.",
        "arguments": [
          {
            "name": "value",
            "description": "The [Zlib](https://www.zlib.net) data to decode.",
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
          "`value` isn't a valid encoded Zlib string."
        ],
        "examples": [
          {
            "title": "Decode Zlib data",
            "source": "decode_zlib!(decode_base64!(\"eJxLzUvOT0mNz00FABI5A6A=\"))",
            "return": "encode_me"
          }
        ],
        "pure": true
      }
    }
  }
}