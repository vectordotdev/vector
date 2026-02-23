{
  "remap": {
    "functions": {
      "ip_pton": {
        "anchor": "ip_pton",
        "name": "ip_pton",
        "category": "IP",
        "description": "Converts IPv4 and IPv6 addresses from text to binary form.\n\n* The binary form of IPv4 addresses is 4 bytes (32 bits) long.\n* The binary form of IPv6 addresses is 16 bytes (128 bits) long.\n\nThis behavior mimics [inet_pton](https://linux.die.net/man/3/inet_pton).",
        "arguments": [
          {
            "name": "value",
            "description": "The IP address (v4 or v6) to convert to binary form.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a valid IP (v4 or v6) address in text form."
        ],
        "examples": [
          {
            "title": "Convert IPv4 address to bytes and encode to Base64",
            "source": "encode_base64(ip_pton!(\"192.168.0.1\"))",
            "return": "wKgAAQ=="
          },
          {
            "title": "Convert IPv6 address to bytes and encode to Base64",
            "source": "encode_base64(ip_pton!(\"2001:db8:85a3::8a2e:370:7334\"))",
            "return": "IAENuIWjAAAAAIouA3BzNA=="
          }
        ],
        "notices": [
          "The binary data from this function is not easily printable. However, functions such as\n`encode_base64` or `encode_percent` can still process it correctly."
        ],
        "pure": true
      }
    }
  }
}