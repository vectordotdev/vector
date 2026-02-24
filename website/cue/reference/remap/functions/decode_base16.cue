{
  "remap": {
    "functions": {
      "decode_base16": {
        "anchor": "decode_base16",
        "name": "decode_base16",
        "category": "Codec",
        "description": "Decodes the `value` (a [Base16](https://en.wikipedia.org/wiki/Hexadecimal) string) into its original string.",
        "arguments": [
          {
            "name": "value",
            "description": "The [Base16](https://en.wikipedia.org/wiki/Hexadecimal) data to decode.",
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
          "`value` isn't a valid encoded Base16 string."
        ],
        "examples": [
          {
            "title": "Decode Base16 data",
            "source": "decode_base16!(\"736F6D6520737472696E672076616C7565\")",
            "return": "some string value"
          },
          {
            "title": "Decode longer Base16 data",
            "source": "decode_base16!(\"796f752068617665207375636365737366756c6c79206465636f646564206d65\")",
            "return": "you have successfully decoded me"
          }
        ],
        "pure": true
      }
    }
  }
}
