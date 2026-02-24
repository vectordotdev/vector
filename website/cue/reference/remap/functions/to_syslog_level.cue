{
  "remap": {
    "functions": {
      "to_syslog_level": {
        "anchor": "to_syslog_level",
        "name": "to_syslog_level",
        "category": "Convert",
        "description": "Converts the `value`, a Syslog [severity level](https://en.wikipedia.org/wiki/Syslog#Severity_level), into its corresponding keyword, i.e. 0 into `\"emerg\"`, 1 into `\"alert\"`, etc.",
        "arguments": [
          {
            "name": "value",
            "description": "The severity level.",
            "required": true,
            "type": [
              "integer"
            ]
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "internal_failure_reasons": [
          "`value` isn't a valid Syslog [severity level](https://en.wikipedia.org/wiki/Syslog#Severity_level)."
        ],
        "examples": [
          {
            "title": "Coerce to a Syslog level",
            "source": "to_syslog_level!(5)",
            "return": "notice"
          },
          {
            "title": "invalid",
            "source": "to_syslog_level!(500)",
            "raises": "function call error for \"to_syslog_level\" at (0:21): severity level 500 not valid"
          }
        ],
        "pure": true
      }
    }
  }
}
