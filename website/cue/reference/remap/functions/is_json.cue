{
  "remap": {
    "functions": {
      "is_json": {
        "anchor": "is_json",
        "name": "is_json",
        "category": "Type",
        "description": "Check if the string is a valid JSON document.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to check if it is a valid JSON document.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "variant",
            "description": "The variant of the JSON type to explicitly check for.",
            "required": false,
            "type": [
              "string"
            ],
            "enum": {
              "number": "Integer or float numbers",
              "bool": "True or false",
              "object": "JSON object - {}",
              "array": "JSON array - []",
              "null": "Exact null value",
              "string": "JSON-formatted string values wrapped with quote marks"
            }
          }
        ],
        "return": {
          "types": [
            "boolean"
          ],
          "rules": [
            "Returns `true` if `value` is a valid JSON document.",
            "Returns `false` if `value` is not JSON-formatted."
          ]
        },
        "examples": [
          {
            "title": "Valid JSON object",
            "source": "is_json(\"{}\")",
            "return": true
          },
          {
            "title": "Non-valid value",
            "source": "is_json(\"{\")",
            "return": false
          },
          {
            "title": "Exact variant",
            "source": "is_json(\"{}\", variant: \"object\")",
            "return": true
          },
          {
            "title": "Non-valid exact variant",
            "source": "is_json(\"{}\", variant: \"array\")",
            "return": false
          },
          {
            "title": "Valid JSON string",
            "source": "is_json(s'\"test\"')",
            "return": true
          }
        ],
        "pure": true
      }
    }
  }
}