{
  "remap": {
    "functions": {
      "decode_percent": {
        "anchor": "decode_percent",
        "name": "decode_percent",
        "category": "Codec",
        "description": "Decodes a [percent-encoded](https://url.spec.whatwg.org/#percent-encoded-bytes) `value` like a URL.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to decode.",
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
            "title": "Percent decode a value",
            "source": "decode_percent(\"foo%20bar%3F\")",
            "return": "foo bar?"
          }
        ],
        "pure": true
      }
    }
  }
}
