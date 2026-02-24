{
  "remap": {
    "functions": {
      "unique": {
        "anchor": "unique",
        "name": "unique",
        "category": "Enumerate",
        "description": "Returns the unique values for an array.\n\nThe first occurrence of each element is kept.",
        "arguments": [
          {
            "name": "value",
            "description": "The array to return unique elements from.",
            "required": true,
            "type": [
              "array"
            ]
          }
        ],
        "return": {
          "types": [
            "array"
          ]
        },
        "examples": [
          {
            "title": "Unique",
            "source": "unique([\"foo\", \"bar\", \"foo\", \"baz\"])",
            "return": [
              "foo",
              "bar",
              "baz"
            ]
          }
        ],
        "pure": true
      }
    }
  }
}
