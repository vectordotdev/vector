{
  "remap": {
    "functions": {
      "to_syslog_facility": {
        "anchor": "to_syslog_facility",
        "name": "to_syslog_facility",
        "category": "Convert",
        "description": "Converts the `value`, a Syslog [facility code](https://en.wikipedia.org/wiki/Syslog#Facility), into its corresponding Syslog keyword. For example, `0` into `\"kern\"`, `1` into `\"user\"`, etc.",
        "arguments": [
          {
            "name": "value",
            "description": "The facility code.",
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
          "`value` is not a valid Syslog [facility code](https://en.wikipedia.org/wiki/Syslog#Facility)."
        ],
        "examples": [
          {
            "title": "Coerce to a Syslog facility",
            "source": "to_syslog_facility!(4)",
            "return": "auth"
          },
          {
            "title": "invalid",
            "source": "to_syslog_facility!(500)",
            "raises": "function call error for \"to_syslog_facility\" at (0:24): facility code 500 not valid"
          }
        ],
        "pure": true
      }
    }
  }
}
