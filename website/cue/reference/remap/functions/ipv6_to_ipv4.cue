{
  "remap": {
    "functions": {
      "ipv6_to_ipv4": {
        "anchor": "ipv6_to_ipv4",
        "name": "ipv6_to_ipv4",
        "category": "IP",
        "description": "Converts the `ip` to an IPv4 address. `ip` is returned unchanged if it's already an IPv4 address. If `ip` is\ncurrently an IPv6 address then it needs to be IPv4 compatible, otherwise an error is thrown.",
        "arguments": [
          {
            "name": "value",
            "description": "The IPv4-mapped IPv6 address to convert.",
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
            "The `ip` is returned unchanged if it's already an IPv4 address. If it's an IPv6 address it must be IPv4\ncompatible, otherwise an error is thrown."
          ]
        },
        "internal_failure_reasons": [
          "`ip` is not a valid IP address.",
          "`ip` is an IPv6 address that is not compatible with IPv4."
        ],
        "examples": [
          {
            "title": "IPv6 to IPv4",
            "source": "ipv6_to_ipv4!(\"::ffff:192.168.0.1\")",
            "return": "192.168.0.1"
          }
        ],
        "pure": true
      }
    }
  }
}