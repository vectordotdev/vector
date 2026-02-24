{
  "remap": {
    "functions": {
      "parse_syslog": {
        "anchor": "parse_syslog",
        "name": "parse_syslog",
        "category": "Parse",
        "description": "Parses the `value` in [Syslog](https://en.wikipedia.org/wiki/Syslog) format.",
        "arguments": [
          {
            "name": "value",
            "description": "The text containing the Syslog message to parse.",
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
        "internal_failure_reasons": [
          "`value` is not a properly formatted Syslog message."
        ],
        "examples": [
          {
            "title": "Parse Syslog log (5424)",
            "source": "parse_syslog!(s'<13>1 2020-03-13T20:45:38.119Z dynamicwireless.name non 2426 ID931 [exampleSDID@32473 iut=\"3\" eventSource= \"Application\" eventID=\"1011\"] Try to override the THX port, maybe it will reboot the neural interface!')",
            "return": {
              "appname": "non",
              "exampleSDID@32473": {
                "eventID": "1011",
                "eventSource": "Application",
                "iut": "3"
              },
              "facility": "user",
              "hostname": "dynamicwireless.name",
              "message": "Try to override the THX port, maybe it will reboot the neural interface!",
              "msgid": "ID931",
              "procid": 2426,
              "severity": "notice",
              "timestamp": "2020-03-13T20:45:38.119Z",
              "version": 1
            }
          }
        ],
        "notices": [
          "The function makes a best effort to parse the various Syslog formats that exists out\nin the wild. This includes [RFC 6587](https://tools.ietf.org/html/rfc6587),\n[RFC 5424](https://tools.ietf.org/html/rfc5424),\n[RFC 3164](https://tools.ietf.org/html/rfc3164), and other common variations (such\nas the Nginx Syslog style).",
          "All values are returned as strings. We recommend manually coercing values to desired types as you see fit."
        ],
        "pure": true
      }
    }
  }
}
