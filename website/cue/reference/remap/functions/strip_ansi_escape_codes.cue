{
  "remap": {
    "functions": {
      "strip_ansi_escape_codes": {
        "anchor": "strip_ansi_escape_codes",
        "name": "strip_ansi_escape_codes",
        "category": "String",
        "description": "Strips [ANSI escape codes](https://en.wikipedia.org/wiki/ANSI_escape_code) from `value`.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to strip.",
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
        "pure": true
      }
    }
  }
}
