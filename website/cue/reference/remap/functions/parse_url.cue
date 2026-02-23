{
  "remap": {
    "functions": {
      "parse_url": {
        "anchor": "parse_url",
        "name": "parse_url",
        "category": "Parse",
        "description": "Parses the `value` in [URL](https://en.wikipedia.org/wiki/URL) format.",
        "arguments": [
          {
            "name": "value",
            "description": "The text of the URL.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "default_known_ports",
            "description": "If true and the port number is not specified in the input URL\nstring (or matches the default port for the scheme), it is\npopulated from well-known ports for the following schemes:\n`http`, `https`, `ws`, `wss`, and `ftp`.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "false"
          }
        ],
        "return": {
          "types": [
            "object"
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a properly formatted URL."
        ],
        "examples": [
          {
            "title": "Parse URL",
            "source": "parse_url!(\"ftp://foo:bar@example.com:4343/foobar?hello=world#123\")",
            "return": {
              "fragment": "123",
              "host": "example.com",
              "password": "bar",
              "path": "/foobar",
              "port": 4343,
              "query": {
                "hello": "world"
              },
              "scheme": "ftp",
              "username": "foo"
            }
          },
          {
            "title": "Parse URL with default port",
            "source": "parse_url!(\"https://example.com\", default_known_ports: true)",
            "return": {
              "fragment": null,
              "host": "example.com",
              "password": "",
              "path": "/",
              "port": 443,
              "query": {},
              "scheme": "https",
              "username": ""
            }
          }
        ],
        "pure": true
      }
    }
  }
}