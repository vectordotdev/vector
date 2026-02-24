{
  "remap": {
    "functions": {
      "parse_json": {
        "anchor": "parse_json",
        "name": "parse_json",
        "category": "Parse",
        "description": "Parses the provided `value` as JSON.\n\nOnly JSON types are returned. If you need to convert a `string` into a `timestamp`,\nconsider the `parse_timestamp` function.",
        "arguments": [
          {
            "name": "value",
            "description": "The string representation of the JSON to parse.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "max_depth",
            "description": "Number of layers to parse for nested JSON-formatted documents.\nThe value must be in the range of 1 to 128.",
            "required": false,
            "type": [
              "integer"
            ]
          },
          {
            "name": "lossy",
            "description": "Whether to parse the JSON in a lossy manner. Replaces invalid UTF-8 characters\nwith the Unicode character `�` (U+FFFD) if set to true, otherwise returns an error\nif there are any invalid UTF-8 characters present.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          }
        ],
        "return": {
          "types": [
            "string",
            "integer",
            "float",
            "boolean",
            "object",
            "array",
            "null"
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a valid JSON-formatted payload."
        ],
        "examples": [
          {
            "title": "Parse JSON",
            "source": "parse_json!(s'{\"key\": \"val\"}')",
            "return": {
              "key": "val"
            }
          },
          {
            "title": "Parse JSON array",
            "source": "parse_json!(\"[true, 0]\")",
            "return": [
              true,
              0
            ]
          },
          {
            "title": "Parse JSON string",
            "source": "parse_json!(s'\"hello\"')",
            "return": "hello"
          },
          {
            "title": "Parse JSON integer",
            "source": "parse_json!(\"42\")",
            "return": 42
          },
          {
            "title": "Parse JSON float",
            "source": "parse_json!(\"42.13\")",
            "return": 42.13
          },
          {
            "title": "Parse JSON boolean",
            "source": "parse_json!(\"false\")",
            "return": false
          },
          {
            "title": "Invalid JSON value",
            "source": "parse_json!(\"{ INVALID }\")",
            "raises": "function call error for \"parse_json\" at (0:26): unable to parse json: key must be a string at line 1 column 3"
          },
          {
            "title": "Parse JSON with max_depth",
            "source": "parse_json!(s'{\"first_level\":{\"second_level\":\"finish\"}}', max_depth: 1)",
            "return": {
              "first_level": "{\"second_level\":\"finish\"}"
            }
          }
        ],
        "notices": [
          "Only JSON types are returned. If you need to convert a `string` into a `timestamp`,\nconsider the [`parse_timestamp`](#parse_timestamp) function."
        ],
        "pure": true
      }
    }
  }
}
