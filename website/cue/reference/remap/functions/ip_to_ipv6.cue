{
  "remap": {
    "functions": {
      "ip_to_ipv6": {
        "anchor": "ip_to_ipv6",
        "name": "ip_to_ipv6",
        "category": "IP",
        "description": "Converts the `ip` to an IPv6 address.",
        "arguments": [
          {
            "name": "value",
            "description": "The IP address to convert to IPv6.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "string"
          ],
          "rules": [
            "The `ip` is returned unchanged if it's already an IPv6 address.",
            "The `ip` is converted to an IPv6 address if it's an IPv4 address."
          ]
        },
        "internal_failure_reasons": [
          "`ip` is not a valid IP address."
        ],
        "examples": [
          {
            "title": "IPv4 to IPv6",
            "source": "ip_to_ipv6!(\"192.168.10.32\")",
            "return": "::ffff:192.168.10.32"
          }
        ],
        "pure": true
      }
    }
  }
}
