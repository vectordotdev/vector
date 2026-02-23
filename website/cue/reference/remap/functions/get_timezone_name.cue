{
  "remap": {
    "functions": {
      "get_timezone_name": {
        "anchor": "get_timezone_name",
        "name": "get_timezone_name",
        "category": "System",
        "description": "Returns the name of the timezone in the Vector configuration (see\n[global configuration options](/docs/reference/configuration/global-options)).\nIf the configuration is set to `local`, then it attempts to\ndetermine the name of the timezone from the host OS. If this\nis not possible, then it returns the fixed offset of the\nlocal timezone for the current time in the format `\"[+-]HH:MM\"`,\nfor example, `\"+02:00\"`.",
        "arguments": [],
        "return": {
          "types": [
            "string"
          ]
        },
        "internal_failure_reasons": [
          "Retrieval of local timezone information failed."
        ],
        "examples": [
          {
            "title": "Get the IANA name of Vector's timezone",
            "source": "get_timezone_name!()",
            "return": "UTC"
          }
        ],
        "pure": true
      }
    }
  }
}