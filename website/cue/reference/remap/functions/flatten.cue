{
  "remap": {
    "functions": {
      "flatten": {
        "anchor": "flatten",
        "name": "flatten",
        "category": "Enumerate",
        "description": "Flattens the `value` into a single-level representation.",
        "arguments": [
          {
            "name": "value",
            "description": "The array or object to flatten.",
            "required": true,
            "type": [
              "object",
              "array"
            ]
          },
          {
            "name": "separator",
            "description": "The separator to join nested keys",
            "required": false,
            "type": [
              "string"
            ],
            "default": "."
          }
        ],
        "return": {
          "types": [
            "object",
            "array"
          ],
          "rules": [
            "The return type matches the `value` type."
          ]
        },
        "examples": [
          {
            "title": "Flatten array",
            "source": "flatten([1, [2, 3, 4], [5, [6, 7], 8], 9])",
            "return": [
              1,
              2,
              3,
              4,
              5,
              6,
              7,
              8,
              9
            ]
          },
          {
            "title": "Flatten object",
            "source": "flatten({\n    \"parent1\": {\n        \"child1\": 1,\n        \"child2\": 2\n    },\n    \"parent2\": {\n        \"child3\": 3\n    }\n})\n",
            "return": {
              "parent1.child1": 1,
              "parent1.child2": 2,
              "parent2.child3": 3
            }
          },
          {
            "title": "Flatten object with custom separator",
            "source": "flatten({ \"foo\": { \"bar\": true }}, \"_\")",
            "return": {
              "foo_bar": true
            }
          }
        ],
        "pure": true
      }
    }
  }
}