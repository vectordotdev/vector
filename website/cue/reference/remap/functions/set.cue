{
  "remap": {
    "functions": {
      "set": {
        "anchor": "set",
        "name": "set",
        "category": "Path",
        "description": "Dynamically insert data into the path of a given object or array.\n\nIf you know the path you want to assign a value to,\nuse static path assignments such as `.foo.bar[1] = true` for\nimproved performance and readability. However, if you do not\nknow the path names, use the dynamic `set` function to\ninsert the data into the object or array.",
        "arguments": [
          {
            "name": "value",
            "description": "The object or array to insert data into.",
            "required": true,
            "type": [
              "object",
              "array"
            ]
          },
          {
            "name": "path",
            "description": "An array of path segments to insert the value into.",
            "required": true,
            "type": [
              "array"
            ]
          },
          {
            "name": "data",
            "description": "The data to be inserted.",
            "required": true,
            "type": [
              "any"
            ]
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
            "source": "set!(value: { \"foo\": \"bar\" }, path: [\"foo\"], data: \"baz\")",
            "return": {
              "foo": "baz"
            }
          },
          {
            "title": "Multi-segment nested field",
            "source": "set!(value: { \"foo\": { \"bar\": \"baz\" } }, path: [\"foo\", \"bar\"], data: \"qux\")",
            "return": {
              "foo": {
                "bar": "qux"
              }
            }
          },
          {
            "title": "Array",
            "source": "set!(value: [\"foo\", \"bar\", \"baz\"], path: [-2], data: 42)",
            "return": [
              "foo",
              42,
              "baz"
            ]
          },
          {
            "title": "Nested fields",
            "source": "set!(value: {}, path: [\"foo\", \"bar\"], data: \"baz\")",
            "return": {
              "foo": {
                "bar": "baz"
              }
            }
          },
          {
            "title": "Nested indexing",
            "source": "set!(value: {\"foo\": { \"bar\": [] }}, path: [\"foo\", \"bar\", 1], data: \"baz\")",
            "return": {
              "foo": {
                "bar": [
                  null,
                  "baz"
                ]
              }
            }
          },
          {
            "title": "External target",
            "source": "set!(value: ., path: [\"bar\"], data: \"baz\")",
            "input": {
              "foo": true
            },
            "return": {
              "foo": true,
              "bar": "baz"
            }
          },
          {
            "title": "Variable",
            "source": "var = { \"foo\": true }\nset!(value: var, path: [\"bar\"], data: \"baz\")\n",
            "return": {
              "foo": true,
              "bar": "baz"
            }
          },
          {
            "title": "Invalid indexing",
            "source": "set!(value: [], path: [\"foo\"], data: \"baz\")",
            "return": {
              "foo": "baz"
            }
          },
          {
            "title": "Invalid segment type",
            "source": "set!({\"foo\": { \"bar\": [92, 42] }}, [\"foo\", true], \"baz\")",
            "raises": "function call error for \"set\" at (0:56): path segment must be either string or integer, not boolean"
          }
        ],
        "pure": true
      }
    }
  }
}
