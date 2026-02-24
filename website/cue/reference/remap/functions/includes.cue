{
  "remap": {
    "functions": {
      "includes": {
        "anchor": "includes",
        "name": "includes",
        "category": "Enumerate",
        "description": "Determines whether the `value` array includes the specified `item`.",
        "arguments": [
          {
            "name": "value",
            "description": "The array.",
            "required": true,
            "type": [
              "array"
            ]
          },
          {
            "name": "item",
            "description": "The item to check.",
            "required": true,
            "type": [
              "any"
            ]
          }
        ],
        "return": {
          "types": [
            "boolean"
          ]
        },
        "examples": [
          {
            "title": "Array includes",
            "source": "includes([\"apple\", \"orange\", \"banana\"], \"banana\")",
            "return": true
          },
          {
            "title": "Includes boolean",
            "source": "includes([1, true], true)",
            "return": true
          },
          {
            "title": "Doesn't include",
            "source": "includes([\"foo\", \"bar\"], \"baz\")",
            "return": false
          }
        ],
        "pure": true
      }
    }
  }
}
