{
  "remap": {
    "functions": {
      "push": {
        "anchor": "push",
        "name": "push",
        "category": "Array",
        "description": "Adds the `item` to the end of the `value` array.",
        "arguments": [
          {
            "name": "value",
            "description": "The target array.",
            "required": true,
            "type": [
              "array"
            ]
          },
          {
            "name": "item",
            "description": "The item to push.",
            "required": true,
            "type": [
              "any"
            ]
          }
        ],
        "return": {
          "types": [
            "array"
          ],
          "rules": [
            "Returns a new array. The `value` is _not_ modified in place."
          ]
        },
        "examples": [
          {
            "title": "Push an item onto an array",
            "source": "push([1, 2], 3)",
            "return": [
              1,
              2,
              3
            ]
          },
          {
            "title": "Empty array",
            "source": "push([], \"bar\")",
            "return": [
              "bar"
            ]
          }
        ],
        "pure": true
      }
    }
  }
}