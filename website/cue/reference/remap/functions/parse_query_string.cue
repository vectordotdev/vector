{
  "remap": {
    "functions": {
      "parse_query_string": {
        "anchor": "parse_query_string",
        "name": "parse_query_string",
        "category": "Parse",
        "description": "Parses the `value` as a query string.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to parse.",
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
        "examples": [
          {
            "title": "Parse simple query string",
            "source": "parse_query_string(\"foo=1&bar=2\")",
            "return": {
              "bar": "2",
              "foo": "1"
            }
          },
          {
            "title": "Parse query string",
            "source": "parse_query_string(\"foo=%2B1&bar=2&bar=3&xyz\")",
            "return": {
              "bar": [
                "2",
                "3"
              ],
              "foo": "+1",
              "xyz": ""
            }
          },
          {
            "title": "Parse Ruby on Rails' query string",
            "source": "parse_query_string(\"?foo%5b%5d=1&foo%5b%5d=2\")",
            "return": {
              "foo[]": [
                "1",
                "2"
              ]
            }
          }
        ],
        "notices": [
          "All values are returned as strings. We recommend manually coercing values to desired\ntypes as you see fit. Empty keys and values are allowed."
        ],
        "pure": true
      }
    }
  }
}