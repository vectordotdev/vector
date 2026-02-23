{
  "remap": {
    "functions": {
      "validate_json_schema": {
        "anchor": "validate_json_schema",
        "name": "validate_json_schema",
        "category": "Type",
        "description": "Check if `value` conforms to a JSON Schema definition. This function validates a JSON payload against a JSON Schema definition. It can be used to ensure that the data structure and types in `value` match the expectations defined in `schema_definition`.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to check if it conforms to the JSON schema definition.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "schema_definition",
            "description": "The location (path) of the JSON Schema definition.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "ignore_unknown_formats",
            "description": "Unknown formats can be silently ignored by setting this to `true` and validation continues without failing due to those fields.",
            "required": false,
            "type": [
              "boolean"
            ]
          }
        ],
        "return": {
          "types": [
            "boolean"
          ],
          "rules": [
            "Returns `true` if `value` conforms to the JSON Schema definition.",
            "Returns `false` if `value` does not conform to the JSON Schema definition."
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a valid JSON Schema payload.",
          "`value` contains custom format declarations and `ignore_unknown_formats` has not been set to `true`.",
          "`schema_definition` is not a valid JSON Schema definition.",
          "`schema_definition` file does not exist."
        ],
        "examples": [
          {
            "title": "Payload contains a valid email",
            "source": "validate_json_schema!(s'{ \"productUser\": \"valid@email.com\" }', \"schema_with_email_format.json\", false)",
            "return": true
          },
          {
            "title": "Payload contains an invalid email",
            "source": "validate_json_schema!(s'{ \"productUser\": \"invalidEmail\" }', \"schema_with_email_format.json\", false)",
            "raises": "function call error for \"validate_json_schema\" at (0:99): JSON schema validation failed: \"invalidEmail\" is not a \"email\" at /productUser"
          },
          {
            "title": "Payload contains a custom format declaration",
            "source": "validate_json_schema!(s'{ \"productUser\": \"a-custom-formatted-string\" }', \"schema_with_custom_format.json\", false)",
            "raises": "function call error for \"validate_json_schema\" at (0:113): Failed to compile schema: Unknown format: 'my-custom-format'. Adjust configuration to ignore unrecognized formats"
          },
          {
            "title": "Payload contains a custom format declaration, with ignore_unknown_formats set to true",
            "source": "validate_json_schema!(s'{ \"productUser\": \"a-custom-formatted-string\" }', \"schema_with_custom_format.json\", true)",
            "return": true
          }
        ],
        "notices": [
          "This function uses a compiled schema cache. The first time it is called with a specific\n`schema_definition`, it will compile the schema and cache it for subsequent calls. This\nimproves performance when validating multiple values against the same schema. The cache\nimplementation is fairly naive and does not support refreshing the schema if it changes.\nIf you update the schema definition file, you must restart Vector to clear the cache."
        ],
        "pure": true
      }
    }
  }
}