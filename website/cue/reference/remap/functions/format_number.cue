{
  "remap": {
    "functions": {
      "format_number": {
        "anchor": "format_number",
        "name": "format_number",
        "category": "Number",
        "description": "Formats the `value` into a string representation of the number.",
        "arguments": [
          {
            "name": "value",
            "description": "The number to format as a string.",
            "required": true,
            "type": [
              "integer",
              "float"
            ]
          },
          {
            "name": "scale",
            "description": "The number of decimal places to display.",
            "required": false,
            "type": [
              "integer"
            ]
          },
          {
            "name": "decimal_separator",
            "description": "The character to use between the whole and decimal parts of the number.",
            "required": false,
            "type": [
              "string"
            ],
            "default": "."
          },
          {
            "name": "grouping_separator",
            "description": "The character to use between each thousands part of the number.",
            "required": false,
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
            "title": "Format a number (3 decimals)",
            "source": "format_number(1234567.89, 3, decimal_separator: \".\", grouping_separator: \",\")",
            "return": "1,234,567.890"
          },
          {
            "title": "Format a number with European-style separators",
            "source": "format_number(4672.4, decimal_separator: \",\", grouping_separator: \"_\")",
            "return": "4_672,4"
          },
          {
            "title": "Format a number with a middle dot separator",
            "source": "format_number(4321.09, 3, decimal_separator: \"·\")",
            "return": "4321·090"
          }
        ],
        "pure": true
      }
    }
  }
}