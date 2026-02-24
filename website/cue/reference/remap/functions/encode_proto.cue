{
  "remap": {
    "functions": {
      "encode_proto": {
        "anchor": "encode_proto",
        "name": "encode_proto",
        "category": "Codec",
        "description": "Encodes the `value` into a protocol buffer payload.",
        "arguments": [
          {
            "name": "value",
            "description": "The object to convert to a protocol buffer payload.",
            "required": true,
            "type": [
              "any"
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
            "string"
          ]
        },
        "internal_failure_reasons": [
          "`desc_file` file does not exist.",
          "`message_type` message type does not exist in the descriptor file."
        ],
        "examples": [
          {
            "title": "Encode to proto",
            "source": "encode_base64(encode_proto!({ \"name\": \"someone\", \"phones\": [{\"number\": \"123456\"}]}, \"test_protobuf.desc\", \"test_protobuf.v1.Person\"))",
            "return": "Cgdzb21lb25lIggKBjEyMzQ1Ng=="
          }
        ],
        "pure": true
      }
    }
  }
}
