{
  "remap": {
    "functions": {
      "parse_key_value": {
        "anchor": "parse_key_value",
        "name": "parse_key_value",
        "category": "Parse",
        "description": "Parses the `value` in key-value format. Also known as [logfmt](https://brandur.org/logfmt).\n\n* Keys and values can be wrapped with `\"`.\n* `\"` characters can be escaped using `\\`.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to parse.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "key_value_delimiter",
            "description": "The string that separates the key from the value.",
            "required": false,
            "type": [
              "string"
            ],
            "default": "="
          },
          {
            "name": "field_delimiter",
            "description": "The string that separates each key-value pair.",
            "required": false,
            "type": [
              "string"
            ],
            "default": " "
          },
          {
            "name": "whitespace",
            "description": "Defines the acceptance of unnecessary whitespace surrounding the configured `key_value_delimiter`.",
            "required": false,
            "type": [
              "string"
            ],
            "enum": {
              "lenient": "Ignore whitespace.",
              "strict": "Parse whitespace as normal character."
            },
            "default": "lenient"
          },
          {
            "name": "accept_standalone_key",
            "description": "Whether a standalone key should be accepted, the resulting object associates such keys with the boolean value `true`.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          }
        ],
        "return": {
          "types": [
            "object"
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a properly formatted key-value string."
        ],
        "examples": [
          {
            "title": "Parse simple key value pairs",
            "source": "parse_key_value!(\"zork=zook zonk=nork\")",
            "return": {
              "zork": "zook",
              "zonk": "nork"
            }
          },
          {
            "title": "Parse logfmt log",
            "source": "parse_key_value!(\n    \"@timestamp=\\\"Sun Jan 10 16:47:39 EST 2021\\\" level=info msg=\\\"Stopping all fetchers\\\" tag#production=stopping_fetchers id=ConsumerFetcherManager-1382721708341 module=kafka.consumer.ConsumerFetcherManager\"\n)\n",
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
            "title": "Parse comma delimited log",
            "source": "parse_key_value!(\n    \"path:\\\"/cart_link\\\", host:store.app.com, fwd: \\\"102.30.171.16\\\", dyno: web.1, connect:0ms, service:87ms, status:304, bytes:632, protocol:https\",\n    field_delimiter: \",\",\n    key_value_delimiter: \":\"\n)\n",
            "return": {
              "path": "/cart_link",
              "host": "store.app.com",
              "fwd": "102.30.171.16",
              "dyno": "web.1",
              "connect": "0ms",
              "service": "87ms",
              "status": "304",
              "bytes": "632",
              "protocol": "https"
            }
          },
          {
            "title": "Parse comma delimited log with standalone keys",
            "source": "parse_key_value!(\n    \"env:prod,service:backend,region:eu-east1,beta\",\n    field_delimiter: \",\",\n    key_value_delimiter: \":\",\n)\n",
            "return": {
              "env": "prod",
              "service": "backend",
              "region": "eu-east1",
              "beta": true
            }
          },
          {
            "title": "Parse duplicate keys",
            "source": "parse_key_value!(\n    \"at=info,method=GET,path=\\\"/index\\\",status=200,tags=dev,tags=dummy\",\n    field_delimiter: \",\",\n    key_value_delimiter: \"=\",\n)\n",
            "return": {
              "at": "info",
              "method": "GET",
              "path": "/index",
              "status": "200",
              "tags": [
                "dev",
                "dummy"
              ]
            }
          },
          {
            "title": "Parse with strict whitespace",
            "source": "parse_key_value!(s'app=my-app ip=1.2.3.4 user= msg=hello-world', whitespace: \"strict\")",
            "return": {
              "app": "my-app",
              "ip": "1.2.3.4",
              "user": "",
              "msg": "hello-world"
            }
          }
        ],
        "notices": [
          "All values are returned as strings or as an array of strings for duplicate keys. We\nrecommend manually coercing values to desired types as you see fit."
        ],
        "pure": true
      }
    }
  }
}
