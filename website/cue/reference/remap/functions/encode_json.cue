{
  "remap": {
    "functions": {
      "encode_json": {
        "anchor": "encode_json",
        "name": "encode_json",
        "category": "Codec",
        "description": "Encodes the `value` to JSON.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to convert to a JSON string.",
            "required": true,
            "type": [
              "any"
            ]
          },
          {
            "name": "pretty",
            "description": "Whether to pretty print the JSON string or not.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "false"
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "examples": [
          {
            "title": "Encode object to JSON",
            "source": "encode_json({\"field\": \"value\", \"another\": [1,2,3]})",
            "return": "s'{\"another\":[1,2,3],\"field\":\"value\"}'"
          },
          {
            "title": "Encode object to as pretty-printed JSON",
            "source": "encode_json({\"field\": \"value\", \"another\": [1,2,3]}, true)",
            "return": "{\n  \"another\": [\n    1,\n    2,\n    3\n  ],\n  \"field\": \"value\"\n}"
          }
        ],
        "pure": true
      }
    }
  }
}