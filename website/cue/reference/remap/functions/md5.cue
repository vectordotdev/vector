{
  "remap": {
    "functions": {
      "md5": {
        "anchor": "md5",
        "name": "md5",
        "category": "Cryptography",
        "description": "Calculates an md5 hash of the `value`.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to calculate the hash for.",
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
            "title": "Create md5 hash",
            "source": "md5(\"foo\")",
            "return": "acbd18db4cc2f85cedef654fccc4a4d8"
          }
        ],
        "pure": true
      }
    }
  }
}