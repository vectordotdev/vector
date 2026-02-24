{
  "remap": {
    "functions": {
      "remove": {
        "anchor": "remove",
        "name": "remove",
        "category": "Path",
        "description": "Dynamically remove the value for a given path.\n\nIf you know the path you want to remove, use\nthe `del` function and static paths such as `del(.foo.bar[1])`\nto remove the value at that path. The `del` function returns the\ndeleted value, and is more performant than `remove`.\nHowever, if you do not know the path names, use the dynamic\n`remove` function to remove the value at the provided path.",
        "arguments": [
          {
            "name": "value",
            "description": "The object or array to remove data from.",
            "required": true,
            "type": [
              "object",
              "array"
            ]
          },
          {
            "name": "path",
            "description": "An array of path segments to remove the value from.",
            "required": true,
            "type": [
              "array"
            ]
          },
          {
            "name": "compact",
            "description": "After deletion, if `compact` is `true`, any empty objects or\narrays left are also removed.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "false"
          }
        ],
        "return": {
          "types": [
            "object",
            "array"
          ]
        },
        "internal_failure_reasons": [
          "The `path` segment must be a string or an integer."
        ],
        "examples": [
          {
            "title": "Single-segment top-level field",
            "source": "remove!(value: { \"foo\": \"bar\" }, path: [\"foo\"])",
            "return": {}
          },
          {
            "title": "Remove unknown field",
            "source": "remove!(value: {\"foo\": \"bar\"}, path: [\"baz\"])",
            "return": {
              "foo": "bar"
            }
          },
          {
            "title": "Multi-segment nested field",
            "source": "remove!(value: { \"foo\": { \"bar\": \"baz\" } }, path: [\"foo\", \"bar\"])",
            "return": {
              "foo": {}
            }
          },
          {
            "title": "Array indexing",
            "source": "remove!(value: [\"foo\", \"bar\", \"baz\"], path: [-2])",
            "return": [
              "foo",
              "baz"
            ]
          },
          {
            "title": "Compaction",
            "source": "remove!(value: { \"foo\": { \"bar\": [42], \"baz\": true } }, path: [\"foo\", \"bar\", 0], compact: true)",
            "return": {
              "foo": {
                "baz": true
              }
            }
          },
          {
            "title": "Compact object",
            "source": "remove!(value: {\"foo\": { \"bar\": true }}, path: [\"foo\", \"bar\"], compact: true)",
            "return": {}
          },
          {
            "title": "Compact array",
            "source": "remove!(value: {\"foo\": [42], \"bar\": true }, path: [\"foo\", 0], compact: true)",
            "return": {
              "bar": true
            }
          },
          {
            "title": "External target",
            "source": "remove!(value: ., path: [\"foo\"])",
            "input": "{ \"foo\": true }",
            "return": {}
          },
          {
            "title": "Variable",
            "source": "var = { \"foo\": true }\nremove!(value: var, path: [\"foo\"])\n",
            "return": {}
          },
          {
            "title": "Missing index",
            "source": "remove!(value: {\"foo\": { \"bar\": [92, 42] }}, path: [\"foo\", \"bar\", 1, -1])",
            "return": {
              "foo": {
                "bar": [
                  92,
                  42
                ]
              }
            }
          },
          {
            "title": "Invalid indexing",
            "source": "remove!(value: [42], path: [\"foo\"])",
            "return": [
              42
            ]
          },
          {
            "title": "Invalid segment type",
            "source": "remove!(value: {\"foo\": { \"bar\": [92, 42] }}, path: [\"foo\", true])",
            "raises": "function call error for \"remove\" at (0:65): path segment must be either string or integer, not boolean"
          }
        ],
        "pure": true
      }
    }
  }
}
