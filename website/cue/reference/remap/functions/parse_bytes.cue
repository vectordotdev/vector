{
  "remap": {
    "functions": {
      "parse_bytes": {
        "anchor": "parse_bytes",
        "name": "parse_bytes",
        "category": "Parse",
        "description": "Parses the `value` into a human-readable bytes format specified by `unit` and `base`.",
        "arguments": [
          {
            "name": "value",
            "description": "The string of the duration with either binary or SI unit.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "unit",
            "description": "The output units for the byte.",
            "required": true,
            "type": [
              "string"
            ],
            "enum": {
              "TiB": "Terabytes (1024 gigabytes)",
              "EB": "Exabytes (1 billion gigabytes in SI)",
              "MiB": "Megabytes (1024 ** 2 bytes)",
              "PiB": "Petabytes (1024 ** 2 gigabytes)",
              "GiB": "Gigabytes (1024 ** 3 bytes)",
              "kB": "Kilobytes (1 thousand bytes in SI)",
              "kiB": "Kilobytes (1024 bytes)",
              "PB": "Petabytes (1 million gigabytes in SI)",
              "MB": "Megabytes (1 million bytes in SI)",
              "GB": "Gigabytes (1 billion bytes in SI)",
              "TB": "Terabytes (1 thousand gigabytes in SI)",
              "B": "Bytes",
              "EiB": "Exabytes (1024 ** 3 gigabytes)"
            }
          },
          {
            "name": "base",
            "description": "The base for the byte, either 2 or 10.",
            "required": false,
            "type": [
              "string"
            ],
            "default": "2"
          }
        ],
        "return": {
          "types": [
            "float"
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a properly formatted bytes."
        ],
        "examples": [
          {
            "title": "Parse bytes (kilobytes)",
            "source": "parse_bytes!(\"1024KiB\", unit: \"MiB\")",
            "return": 1.0
          },
          {
            "title": "Parse kilobytes in default binary units",
            "source": "parse_bytes!(\"1KiB\", unit: \"B\")",
            "return": 1024.0
          },
          {
            "title": "Parse bytes in SI unit (terabytes)",
            "source": "parse_bytes!(\"4TB\", unit: \"MB\", base: \"10\")",
            "return": 4000000.0
          },
          {
            "title": "Parse gigabytes in decimal units",
            "source": "parse_bytes!(\"1GB\", unit: \"B\", base: \"10\")",
            "return": 1000000000.0
          },
          {
            "title": "Parse bytes in ambiguous unit (gigabytes)",
            "source": "parse_bytes!(\"1GB\", unit: \"B\", base: \"2\")",
            "return": 1073741824.0
          },
          {
            "title": "Parse gigabytes in ambiguous decimal units",
            "source": "parse_bytes!(\"1GB\", unit: \"MB\", base: \"2\")",
            "return": 1024.0
          }
        ],
        "pure": true
      }
    }
  }
}