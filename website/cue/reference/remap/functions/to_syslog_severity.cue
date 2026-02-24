{
  "remap": {
    "functions": {
      "to_syslog_severity": {
        "anchor": "to_syslog_severity",
        "name": "to_syslog_severity",
        "category": "Convert",
        "description": "Converts the `value`, a Syslog [log level keyword](https://en.wikipedia.org/wiki/Syslog#Severity_level), into a Syslog integer severity level (`0` to `7`).",
        "arguments": [
          {
            "name": "value",
            "description": "The Syslog level keyword to convert.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "integer"
          ],
          "rules": [
            "The now-deprecated keywords `panic`, `error`, and `warn` are converted to `0`, `3`, and `4` respectively."
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a valid Syslog level keyword."
        ],
        "examples": [
          {
            "title": "Coerce to Syslog severity",
            "source": "to_syslog_severity!(\"alert\")",
            "return": 1
          },
          {
            "title": "invalid",
            "source": "to_syslog_severity!(s'foobar')",
            "raises": "function call error for \"to_syslog_severity\" at (0:30): syslog level foobar not valid"
          }
        ],
        "pure": true
      }
    }
  }
}
