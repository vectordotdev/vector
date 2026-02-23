{
  "remap": {
    "functions": {
      "slice": {
        "anchor": "slice",
        "name": "slice",
        "category": "String",
        "description": "Returns a slice of `value` between the `start` and `end` positions.\n\nIf the `start` and `end` parameters are negative, they refer to positions counting from the right of the\nstring or array. If `end` refers to a position that is greater than the length of the string or array,\na slice up to the end of the string or array is returned.",
        "arguments": [
          {
            "name": "value",
            "description": "The string or array to slice.",
            "required": true,
            "type": [
              "string",
              "array"
            ]
          },
          {
            "name": "start",
            "description": "The inclusive start position. A zero-based index that can be negative.",
            "required": true,
            "type": [
              "integer"
            ]
          },
          {
            "name": "end",
            "description": "The exclusive end position. A zero-based index that can be negative.",
            "required": false,
            "type": [
              "integer"
            ],
            "default": "String length"
          }
        ],
        "return": {
          "types": [
            "string",
            "array"
          ]
        },
        "examples": [
          {
            "title": "Slice a string (positive index)",
            "source": "slice!(\"Supercalifragilisticexpialidocious\", start: 5, end: 13)",
            "return": "califrag"
          },
          {
            "title": "Slice a string (negative index)",
            "source": "slice!(\"Supercalifragilisticexpialidocious\", start: 5, end: -14)",
            "return": "califragilistic"
          },
          {
            "title": "String start",
            "source": "slice!(\"foobar\", 3)",
            "return": "bar"
          },
          {
            "title": "Array start",
            "source": "slice!([0, 1, 2], 1)",
            "return": [
              1,
              2
            ]
          }
        ],
        "pure": true
      }
    }
  }
}