{
  "remap": {
    "functions": {
      "parse_xml": {
        "anchor": "parse_xml",
        "name": "parse_xml",
        "category": "Parse",
        "description": "Parses the `value` as XML.",
        "arguments": [
          {
            "name": "value",
            "description": "The string representation of the XML document to parse.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "trim",
            "description": "Remove excess whitespace between XML elements.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          },
          {
            "name": "include_attr",
            "description": "Include XML tag attributes in the returned object.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          },
          {
            "name": "attr_prefix",
            "description": "String prefix to use for XML tag attribute keys.",
            "required": false,
            "type": [
              "string"
            ],
            "default": "@"
          },
          {
            "name": "text_key",
            "description": "Key name to use for expanded text nodes.",
            "required": false,
            "type": [
              "string"
            ],
            "default": "text"
          },
          {
            "name": "always_use_text_key",
            "description": "Always return text nodes as `{\"<text_key>\": \"value\"}.`",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "false"
          },
          {
            "name": "parse_bool",
            "description": "Parse \"true\" and \"false\" as boolean.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          },
          {
            "name": "parse_null",
            "description": "Parse \"null\" as null.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          },
          {
            "name": "parse_number",
            "description": "Parse numbers as integers/floats.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          }
        ],
        "return": {
          "types": [
            "object"
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a valid XML document."
        ],
        "examples": [
          {
            "title": "Parse XML",
            "source": "value = s'<book category=\"CHILDREN\"><title lang=\"en\">Harry Potter</title><author>J K. Rowling</author><year>2005</year></book>';\n\nparse_xml!(value, text_key: \"value\", parse_number: false)\n",
            "return": {
              "book": {
                "@category": "CHILDREN",
                "author": "J K. Rowling",
                "title": {
                  "@lang": "en",
                  "value": "Harry Potter"
                },
                "year": "2005"
              }
            }
          }
        ],
        "notices": [
          "Valid XML must contain exactly one root node. Always returns an object."
        ],
        "pure": true
      }
    }
  }
}
