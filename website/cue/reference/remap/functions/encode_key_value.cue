{
  "remap": {
    "functions": {
      "encode_key_value": {
        "anchor": "encode_key_value",
        "name": "encode_key_value",
        "category": "Codec",
        "description": "Encodes the `value` into key-value format with customizable delimiters. Default delimiters match the [logfmt](https://brandur.org/logfmt) format.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to convert to a string.",
            "required": true,
            "type": [
              "object"
            ]
          },
          {
            "name": "fields_ordering",
            "description": "The ordering of fields to preserve. Any fields not in this list are listed unordered, after all ordered fields.",
            "required": false,
            "type": [
              "array"
            ],
            "default": "[]"
          },
          {
            "name": "key_value_delimiter",
            "description": "The string that separates the key from the value.",
            "required": false,
            "type": [
              "string"
            ],
            "default": "="
          },
          {
            "name": "field_delimiter",
            "description": "The string that separates each key-value pair.",
            "required": false,
            "type": [
              "string"
            ],
            "default": " "
          },
          {
            "name": "flatten_boolean",
            "description": "Whether to encode key-value with a boolean value as a standalone key if `true` and nothing if `false`.",
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
        "internal_failure_reasons": [
          "`fields_ordering` contains a non-string element."
        ],
        "examples": [
          {
            "title": "Encode with default delimiters (no ordering)",
            "source": "encode_key_value(\n    {\n        \"ts\": \"2021-06-05T17:20:00Z\",\n        \"msg\": \"This is a message\",\n        \"lvl\": \"info\"\n    }\n)\n",
            "return": "lvl=info msg=\"This is a message\" ts=2021-06-05T17:20:00Z"
          },
          {
            "title": "Encode with default delimiters (fields ordering)",
            "source": "encode_key_value!(\n    {\n        \"ts\": \"2021-06-05T17:20:00Z\",\n        \"msg\": \"This is a message\",\n        \"lvl\": \"info\",\n        \"log_id\": 12345\n    },\n    [\"ts\", \"lvl\", \"msg\"]\n)\n",
            "return": "ts=2021-06-05T17:20:00Z lvl=info msg=\"This is a message\" log_id=12345"
          },
          {
            "title": "Encode with default delimiters (nested fields)",
            "source": "encode_key_value(\n    {\n        \"agent\": {\"name\": \"foo\"},\n        \"log\": {\"file\": {\"path\": \"my.log\"}},\n        \"event\": \"log\"\n    }\n)\n",
            "return": "agent.name=foo event=log log.file.path=my.log"
          },
          {
            "title": "Encode with default delimiters (nested fields ordering)",
            "source": "encode_key_value!(\n    {\n        \"agent\": {\"name\": \"foo\"},\n        \"log\": {\"file\": {\"path\": \"my.log\"}},\n        \"event\": \"log\"\n    },\n    [\"event\", \"log.file.path\", \"agent.name\"])\n",
            "return": "event=log log.file.path=my.log agent.name=foo"
          },
          {
            "title": "Encode with custom delimiters (no ordering)",
            "source": "encode_key_value(\n    {\"ts\": \"2021-06-05T17:20:00Z\", \"msg\": \"This is a message\", \"lvl\": \"info\"},\n    field_delimiter: \",\",\n    key_value_delimiter: \":\"\n)\n",
            "return": "lvl:info,msg:\"This is a message\",ts:2021-06-05T17:20:00Z"
          },
          {
            "title": "Encode with custom delimiters and flatten boolean",
            "source": "encode_key_value(\n    {\"ts\": \"2021-06-05T17:20:00Z\", \"msg\": \"This is a message\", \"lvl\": \"info\", \"beta\": true, \"dropped\": false},\n    field_delimiter: \",\",\n    key_value_delimiter: \":\",\n    flatten_boolean: true\n)\n",
            "return": "beta,lvl:info,msg:\"This is a message\",ts:2021-06-05T17:20:00Z"
          }
        ],
        "notices": [
          "If `fields_ordering` is specified then the function is fallible else it is infallible."
        ],
        "pure": true
      }
    }
  }
}
