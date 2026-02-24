{
  "remap": {
    "functions": {
      "decode_gzip": {
        "anchor": "decode_gzip",
        "name": "decode_gzip",
        "category": "Codec",
        "description": "Decodes the `value` (a [Gzip](https://www.gzip.org/) string) into its original string.",
        "arguments": [
          {
            "name": "value",
            "description": "The [Gzip](https://www.gzip.org/) data to decode.",
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
          "`value` isn't a valid encoded Gzip string."
        ],
        "examples": [
          {
            "title": "Decode Gzip data",
            "source": "decode_gzip!(decode_base64!(\"H4sIAB8BymMAAyvISU0sTlVISU3OT0lVyE0FAJsZ870QAAAA\"))",
            "return": "please decode me"
          }
        ],
        "pure": true
      }
    }
  }
}
