{
  "remap": {
    "functions": {
      "upcase": {
        "anchor": "upcase",
        "name": "upcase",
        "category": "String",
        "description": "Upcases `value`, where upcase is defined according to the Unicode Derived Core Property Uppercase.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to convert to uppercase.",
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
            "title": "Upcase a string",
            "source": "upcase(\"Hello, World!\")",
            "return": "HELLO, WORLD!"
          }
        ],
        "pure": true
      }
    }
  }
}
