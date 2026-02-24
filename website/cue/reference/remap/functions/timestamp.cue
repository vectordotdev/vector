{
  "remap": {
    "functions": {
      "timestamp": {
        "anchor": "timestamp",
        "name": "timestamp",
        "category": "Type",
        "description": "Returns `value` if it is a timestamp, otherwise returns an error. This enables the type checker to guarantee that the returned value is a timestamp and can be used in any function that expects a timestamp.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to check if it is a timestamp.",
            "required": true,
            "type": [
              "any"
            ]
          }
        ],
        "return": {
          "types": [
            "timestamp"
          ],
          "rules": [
            "Returns the `value` if it's a timestamp.",
            "Raises an error if not a timestamp."
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a timestamp."
        ],
        "examples": [
          {
            "title": "Declare a timestamp type",
            "source": "timestamp(t'2020-10-10T16:00:00Z')",
            "return": "t'2020-10-10T16:00:00Z'"
          },
          {
            "title": "Invalid type",
            "source": "timestamp!(true)",
            "raises": "function call error for \"timestamp\" at (0:16): expected timestamp, got boolean"
          }
        ],
        "pure": true
      }
    }
  }
}
