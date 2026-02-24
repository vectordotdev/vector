{
  "remap": {
    "functions": {
      "encode_percent": {
        "anchor": "encode_percent",
        "name": "encode_percent",
        "category": "Codec",
        "description": "Encodes a `value` with [percent encoding](https://url.spec.whatwg.org/#percent-encoded-bytes) to safely be used in URLs.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to encode.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "ascii_set",
            "description": "The ASCII set to use when encoding the data.",
            "required": false,
            "type": [
              "string"
            ],
            "enum": {
              "NON_ALPHANUMERIC": "Encode any non-alphanumeric characters. This is the safest option.",
              "CONTROLS": "Encode only [control characters](https://infra.spec.whatwg.org/#c0-control).",
              "FRAGMENT": "Encode only [fragment characters](https://url.spec.whatwg.org/#fragment-percent-encode-set)",
              "QUERY": "Encode only [query characters](https://url.spec.whatwg.org/#query-percent-encode-set)",
              "SPECIAL": "Encode only [special characters](https://url.spec.whatwg.org/#special-percent-encode-set)",
              "PATH": "Encode only [path characters](https://url.spec.whatwg.org/#path-percent-encode-set)",
              "USERINFO": "Encode only [userinfo characters](https://url.spec.whatwg.org/#userinfo-percent-encode-set)",
              "COMPONENT": "Encode only [component characters](https://url.spec.whatwg.org/#component-percent-encode-set)",
              "WWW_FORM_URLENCODED": "Encode only [`application/x-www-form-urlencoded`](https://url.spec.whatwg.org/#application-x-www-form-urlencoded-percent-encode-set)"
            },
            "default": "NON_ALPHANUMERIC"
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "examples": [
          {
            "title": "Percent encode all non-alphanumeric characters (default)",
            "source": "encode_percent(\"foo bar?\")",
            "return": "foo%20bar%3F"
          },
          {
            "title": "Percent encode only control characters",
            "source": "encode_percent(\"foo \\tbar\", ascii_set: \"CONTROLS\")",
            "return": "foo %09bar"
          },
          {
            "title": "Percent encode special characters",
            "source": "encode_percent(\"foo@bar?\")",
            "return": "foo%40bar%3F"
          }
        ],
        "pure": true
      }
    }
  }
}
