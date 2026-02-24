{
  "remap": {
    "functions": {
      "parse_logfmt": {
        "anchor": "parse_logfmt",
        "name": "parse_logfmt",
        "category": "Parse",
        "description": "Parses the `value` in [logfmt](https://brandur.org/logfmt).\n\n* Keys and values can be wrapped using the `\"` character.\n* `\"` characters can be escaped by the `\\` character.\n* As per this [logfmt specification](https://pkg.go.dev/github.com/kr/logfmt#section-documentation), the `parse_logfmt` function accepts standalone keys and assigns them a Boolean value of `true`.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to parse.",
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
          "`value` is not a properly formatted key-value string"
        ],
        "examples": [
          {
            "title": "Parse simple logfmt log",
            "source": "parse_logfmt!(\"zork=zook zonk=nork\")",
            "return": {
              "zork": "zook",
              "zonk": "nork"
            }
          },
          {
            "title": "Parse logfmt log",
            "source": "parse_logfmt!(\n    \"@timestamp=\\\"Sun Jan 10 16:47:39 EST 2021\\\" level=info msg=\\\"Stopping all fetchers\\\" tag#production=stopping_fetchers id=ConsumerFetcherManager-1382721708341 module=kafka.consumer.ConsumerFetcherManager\"\n)\n",
            "return": {
              "@timestamp": "Sun Jan 10 16:47:39 EST 2021",
              "level": "info",
              "msg": "Stopping all fetchers",
              "tag#production": "stopping_fetchers",
              "id": "ConsumerFetcherManager-1382721708341",
              "module": "kafka.consumer.ConsumerFetcherManager"
            }
          },
          {
            "title": "Parse logfmt log with standalone key",
            "source": "parse_logfmt!(\"zork=zook plonk zonk=nork\")",
            "return": {
              "plonk": true,
              "zork": "zook",
              "zonk": "nork"
            }
          }
        ],
        "pure": true
      }
    }
  }
}
