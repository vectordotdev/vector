{
  "remap": {
    "functions": {
      "get": {
        "anchor": "get",
        "name": "get",
        "category": "Path",
        "description": "Dynamically get the value of a given path.\n\nIf you know the path you want to look up, use\nstatic paths such as `.foo.bar[1]` to get the value of that\npath. However, if you do not know the path names,\nuse the dynamic `get` function to get the requested value.",
        "arguments": [
          {
            "name": "value",
            "description": "The object or array to query.",
            "required": true,
            "type": [
              "object",
              "array"
            ]
          },
          {
            "name": "path",
            "description": "An array of path segments to look for the value.",
            "required": true,
            "type": [
              "array"
            ]
          }
        ],
        "return": {
          "types": [
            "any"
          ]
        },
        "internal_failure_reasons": [
          "The `path` segment must be a string or an integer."
        ],
        "examples": [
          {
            "title": "Single-segment top-level field",
            "source": "get!(value: {\"foo\": \"bar\"}, path: [\"foo\"])",
            "return": "bar"
          },
          {
            "title": "Returns null for unknown field",
            "source": "get!(value: {\"foo\": \"bar\"}, path: [\"baz\"])",
            "return": null
          },
          {
            "title": "Multi-segment nested field",
            "source": "get!(value: {\"foo\": { \"bar\": true }}, path: [\"foo\", \"bar\"])",
            "return": true
          },
          {
            "title": "Array indexing",
            "source": "get!(value: [92, 42], path: [0])",
            "return": 92
          },
          {
            "title": "Array indexing (negative)",
            "source": "get!(value: [\"foo\", \"bar\", \"baz\"], path: [-2])",
            "return": "bar"
          },
          {
            "title": "Nested indexing",
            "source": "get!(value: {\"foo\": { \"bar\": [92, 42] }}, path: [\"foo\", \"bar\", 1])",
            "return": 42
          },
          {
            "title": "External target",
            "source": ". = { \"foo\": true }\nget!(value: ., path: [\"foo\"])\n",
            "return": true
          },
          {
            "title": "Variable",
            "source": "var = { \"foo\": true }\nget!(value: var, path: [\"foo\"])\n",
            "return": true
          },
          {
            "title": "Missing index",
            "source": "get!(value: {\"foo\": { \"bar\": [92, 42] }}, path: [\"foo\", \"bar\", 1, -1])",
            "return": null
          },
          {
            "title": "Invalid indexing",
            "source": "get!(value: [42], path: [\"foo\"])",
            "return": null
          },
          {
            "title": "Invalid segment type",
            "source": "get!(value: {\"foo\": { \"bar\": [92, 42] }}, path: [\"foo\", true])",
            "raises": "function call error for \"get\" at (0:62): path segment must be either string or integer, not boolean"
          }
        ],
        "pure": true
      }
    }
  }
}