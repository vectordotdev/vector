{
  "remap": {
    "functions": {
      "match_datadog_query": {
        "anchor": "match_datadog_query",
        "name": "match_datadog_query",
        "category": "Object",
        "description": "Matches an object against a [Datadog Search Syntax](https://docs.datadoghq.com/logs/explorer/search_syntax/) query.",
        "arguments": [
          {
            "name": "value",
            "description": "The object.",
            "required": true,
            "type": [
              "object"
            ]
          },
          {
            "name": "query",
            "description": "The Datadog Search Syntax query.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "boolean"
          ]
        },
        "examples": [
          {
            "title": "OR query",
            "source": "match_datadog_query({\"message\": \"contains this and that\"}, \"this OR that\")",
            "return": true
          },
          {
            "title": "AND query",
            "source": "match_datadog_query({\"message\": \"contains only this\"}, \"this AND that\")",
            "return": false
          },
          {
            "title": "Attribute wildcard",
            "source": "match_datadog_query({\"name\": \"foobar\"}, \"@name:foo*\")",
            "return": true
          },
          {
            "title": "Tag range",
            "source": "match_datadog_query({\"tags\": [\"a:x\", \"b:y\", \"c:z\"]}, s'b:[\"x\" TO \"z\"]')",
            "return": true
          }
        ],
        "pure": true
      }
    }
  }
}