{
  "remap": {
    "functions": {
      "tally_value": {
        "anchor": "tally_value",
        "name": "tally_value",
        "category": "Enumerate",
        "description": "Counts the number of times a specific value appears in the provided array.",
        "arguments": [
          {
            "name": "array",
            "description": "The array to search through.",
            "required": true,
            "type": [
              "array"
            ]
          },
          {
            "name": "value",
            "description": "The value to count occurrences of in the array.",
            "required": true,
            "type": [
              "any"
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
            "title": "count matching values",
            "source": "tally_value([\"foo\", \"bar\", \"foo\", \"baz\"], \"foo\")",
            "return": 2
          }
        ],
        "pure": true
      }
    }
  }
}