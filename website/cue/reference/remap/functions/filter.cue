{
  "remap": {
    "functions": {
      "filter": {
        "anchor": "filter",
        "name": "filter",
        "category": "Enumerate",
        "description": "Filter elements from a collection.\n\nThis function currently *does not* support recursive iteration.\n\nThe function uses the function closure syntax to allow reading\nthe key-value or index-value combination for each item in the\ncollection.\n\nThe same scoping rules apply to closure blocks as they do for\nregular blocks. This means that any variable defined in parent scopes\nis accessible, and mutations to those variables are preserved,\nbut any new variables instantiated in the closure block are\nunavailable outside of the block.\n\nSee the examples below to learn about the closure syntax.",
        "arguments": [
          {
            "name": "value",
            "description": "The array or object to filter.",
            "required": true,
            "type": [
              "object",
              "array"
            ]
          }
        ],
        "return": {
          "types": [
            "object",
            "array"
          ]
        },
        "examples": [
          {
            "title": "Filter elements",
            "source": ". = { \"tags\": [\"foo\", \"bar\", \"foo\", \"baz\"] }\nfilter(array(.tags)) -> |_index, value| {\n    value != \"foo\"\n}\n",
            "return": [
              "bar",
              "baz"
            ]
          },
          {
            "title": "Filter object",
            "source": "filter({ \"a\": 1, \"b\": 2 }) -> |key, _value| { key == \"a\" }",
            "return": {
              "a": 1
            }
          },
          {
            "title": "Filter array",
            "source": "filter([1, 2]) -> |_index, value| { value < 2 }",
            "return": [
              1
            ]
          }
        ],
        "pure": true
      }
    }
  }
}
