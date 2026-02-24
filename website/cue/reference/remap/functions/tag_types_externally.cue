{
  "remap": {
    "functions": {
      "tag_types_externally": {
        "anchor": "tag_types_externally",
        "name": "tag_types_externally",
        "category": "Type",
        "description": "Adds type information to all (nested) scalar values in the provided `value`.\n\nThe type information is added externally, meaning that `value` has the form of `\"type\": value` after this\ntransformation.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to tag with types.",
            "required": true,
            "type": [
              "any"
            ]
          }
        ],
        "return": {
          "types": [
            "object",
            "array",
            "null"
          ]
        },
        "examples": [
          {
            "title": "Tag types externally (scalar)",
            "source": "tag_types_externally(123)",
            "return": {
              "integer": 123
            }
          },
          {
            "title": "Tag types externally (object)",
            "source": "tag_types_externally({\n    \"message\": \"Hello world\",\n    \"request\": {\n        \"duration_ms\": 67.9\n    }\n})\n",
            "return": {
              "message": {
                "string": "Hello world"
              },
              "request": {
                "duration_ms": {
                  "float": 67.9
                }
              }
            }
          },
          {
            "title": "Tag types externally (array)",
            "source": "tag_types_externally([\"foo\", \"bar\"])",
            "return": [
              {
                "string": "foo"
              },
              {
                "string": "bar"
              }
            ]
          },
          {
            "title": "Tag types externally (null)",
            "source": "tag_types_externally(null)",
            "return": null
          }
        ],
        "pure": true
      }
    }
  }
}
