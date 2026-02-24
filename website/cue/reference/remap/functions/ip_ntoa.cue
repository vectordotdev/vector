{
  "remap": {
    "functions": {
      "ip_ntoa": {
        "anchor": "ip_ntoa",
        "name": "ip_ntoa",
        "category": "IP",
        "description": "Converts numeric representation of IPv4 address in network-order bytes\nto numbers-and-dots notation.\n\nThis behavior mimics [inet_ntoa](https://linux.die.net/man/3/inet_ntoa).",
        "arguments": [
          {
            "name": "value",
            "description": "The integer representation of an IPv4 address.",
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
          "`value` cannot fit in an unsigned 32-bit integer."
        ],
        "examples": [
          {
            "title": "Integer to IPv4",
            "source": "ip_ntoa!(16909060)",
            "return": "1.2.3.4"
          }
        ],
        "pure": true
      }
    }
  }
}
