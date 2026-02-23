{
  "remap": {
    "functions": {
      "strlen": {
        "anchor": "strlen",
        "name": "strlen",
        "category": "Enumerate",
        "description": "Returns the number of UTF-8 characters in `value`. This differs from\n`length` which counts the number of bytes of a string.\n\n**Note**: This is the count of [Unicode scalar values](https://www.unicode.org/glossary/#unicode_scalar_value)\nwhich can sometimes differ from [Unicode code points](https://www.unicode.org/glossary/#code_point).",
        "arguments": [
          {
            "name": "value",
            "description": "The string.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "integer"
          ]
        },
        "examples": [
          {
            "title": "Count Unicode scalar values",
            "source": "strlen(\"ñandú\")",
            "return": 5
          }
        ],
        "pure": true
      }
    }
  }
}