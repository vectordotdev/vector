{
  "remap": {
    "functions": {
      "parse_proto": {
        "anchor": "parse_proto",
        "name": "parse_proto",
        "category": "Parse",
        "description": "Parses the `value` as a protocol buffer payload.",
        "arguments": [
          {
            "name": "value",
            "description": "The protocol buffer payload to parse.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "desc_file",
            "description": "The path to the protobuf descriptor set file. Must be a literal string.\n\nThis file is the output of protoc -o <path> ...",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "message_type",
            "description": "The name of the message type to use for serializing.\n\nMust be a literal string.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "object"
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a valid proto payload.",
          "`desc_file` file does not exist.",
          "`message_type` message type does not exist in the descriptor file."
        ],
        "examples": [
          {
            "title": "Parse proto",
            "source": "parse_proto!(decode_base64!(\"Cgdzb21lb25lIggKBjEyMzQ1Ng==\"), \"test_protobuf.desc\", \"test_protobuf.v1.Person\")",
            "return": {
              "name": "someone",
              "phones": [
                {
                  "number": "123456"
                }
              ]
            }
          }
        ],
        "notices": [
          "Only proto messages are parsed and returned."
        ],
        "pure": true
      }
    }
  }
}
