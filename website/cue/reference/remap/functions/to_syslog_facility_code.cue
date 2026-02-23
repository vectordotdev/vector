{
  "remap": {
    "functions": {
      "to_syslog_facility_code": {
        "anchor": "to_syslog_facility_code",
        "name": "to_syslog_facility_code",
        "category": "Convert",
        "description": "Converts the `value`, a Syslog [facility keyword](https://en.wikipedia.org/wiki/Syslog#Facility), into a Syslog integer facility code (`0` to `23`).",
        "arguments": [
          {
            "name": "value",
            "description": "The Syslog facility keyword to convert.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "integer"
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a valid Syslog facility keyword."
        ],
        "examples": [
          {
            "title": "Coerce to Syslog facility code",
            "source": "to_syslog_facility_code!(\"authpriv\")",
            "return": 10
          },
          {
            "title": "invalid",
            "source": "to_syslog_facility_code!(s'foobar')",
            "raises": "function call error for \"to_syslog_facility_code\" at (0:35): syslog facility 'foobar' not valid"
          }
        ],
        "pure": true
      }
    }
  }
}