{
  "remap": {
    "functions": {
      "downcase": {
        "anchor": "downcase",
        "name": "downcase",
        "category": "String",
        "description": "Downcases the `value` string, where downcase is defined according to the Unicode Derived Core Property Lowercase.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to convert to lowercase.",
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
            "title": "Downcase a string",
            "source": "downcase(\"Hello, World!\")",
            "return": "hello, world!"
          },
          {
            "title": "Downcase with number",
            "source": "downcase(\"FOO 2 BAR\")",
            "return": "foo 2 bar"
          }
        ],
        "pure": true
      }
    }
  }
}
