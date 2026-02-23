{
  "remap": {
    "functions": {
      "parse_cbor": {
        "anchor": "parse_cbor",
        "name": "parse_cbor",
        "category": "Parse",
        "description": "Parses the `value` as [CBOR](https://cbor.io).",
        "arguments": [
          {
            "name": "value",
            "description": "The CBOR payload to parse.",
            "required": true,
            "type": [
              "string"
            ]
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
          "`value` is not a valid CBOR-formatted payload."
        ],
        "examples": [
          {
            "title": "Parse CBOR",
            "source": "parse_cbor!(decode_base64!(\"oWVmaWVsZGV2YWx1ZQ==\"))",
            "return": {
              "field": "value"
            }
          },
          {
            "title": "array",
            "source": "parse_cbor!(decode_base64!(\"gvUA\"))",
            "return": [
              true,
              0
            ]
          },
          {
            "title": "string",
            "source": "parse_cbor!(decode_base64!(\"ZWhlbGxv\"))",
            "return": "hello"
          },
          {
            "title": "integer",
            "source": "parse_cbor!(decode_base64!(\"GCo=\"))",
            "return": 42
          },
          {
            "title": "float",
            "source": "parse_cbor!(decode_base64!(\"+0BFEKPXCj1x\"))",
            "return": 42.13
          },
          {
            "title": "boolean",
            "source": "parse_cbor!(decode_base64!(\"9A==\"))",
            "return": false
          }
        ],
        "notices": [
          "Only CBOR types are returned."
        ],
        "pure": true
      }
    }
  }
}