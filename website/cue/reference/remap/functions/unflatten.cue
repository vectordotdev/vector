{
  "remap": {
    "functions": {
      "unflatten": {
        "anchor": "unflatten",
        "name": "unflatten",
        "category": "Enumerate",
        "description": "Unflattens the `value` into a nested representation.",
        "arguments": [
          {
            "name": "value",
            "description": "The array or object to unflatten.",
            "required": true,
            "type": [
              "object"
            ]
          },
          {
            "name": "separator",
            "description": "The separator to split flattened keys.",
            "required": false,
            "type": [
              "string"
            ],
            "default": "."
          },
          {
            "name": "recursive",
            "description": "Whether to recursively unflatten the object values.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          }
        ],
        "return": {
          "types": [
            "object"
          ]
        },
        "examples": [
          {
            "title": "Unflatten",
            "source": "unflatten({\n    \"foo.bar.baz\": true,\n    \"foo.bar.qux\": false,\n    \"foo.quux\": 42\n})\n",
            "return": {
              "foo": {
                "bar": {
                  "baz": true,
                  "qux": false
                },
                "quux": 42
              }
            }
          },
          {
            "title": "Unflatten recursively",
            "source": "unflatten({\n    \"flattened.parent\": {\n        \"foo.bar\": true,\n        \"foo.baz\": false\n    }\n})\n",
            "return": {
              "flattened": {
                "parent": {
                  "foo": {
                    "bar": true,
                    "baz": false
                  }
                }
              }
            }
          },
          {
            "title": "Unflatten non-recursively",
            "source": "unflatten({\n    \"flattened.parent\": {\n        \"foo.bar\": true,\n        \"foo.baz\": false\n    }\n}, recursive: false)\n",
            "return": {
              "flattened": {
                "parent": {
                  "foo.bar": true,
                  "foo.baz": false
                }
              }
            }
          },
          {
            "title": "Ignore inconsistent keys values",
            "source": "unflatten({\n    \"a\": 3,\n    \"a.b\": 2,\n    \"a.c\": 4\n})\n",
            "return": {
              "a": {
                "b": 2,
                "c": 4
              }
            }
          },
          {
            "title": "Unflatten with custom separator",
            "source": "unflatten({ \"foo_bar\": true }, \"_\")",
            "return": {
              "foo": {
                "bar": true
              }
            }
          }
        ],
        "pure": true
      }
    }
  }
}