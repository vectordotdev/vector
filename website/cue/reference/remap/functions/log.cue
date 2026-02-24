{
  "remap": {
    "functions": {
      "log": {
        "anchor": "log",
        "name": "log",
        "category": "Debug",
        "description": "Logs the `value` to [stdout](https://en.wikipedia.org/wiki/Standard_streams#Standard_output_(stdout)) at the specified `level`.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to log.",
            "required": true,
            "type": [
              "any"
            ]
          },
          {
            "name": "level",
            "description": "The log level.",
            "required": false,
            "type": [
              "string"
            ],
            "enum": {
              "warn": "Log at the `warn` level.",
              "info": "Log at the `info` level.",
              "debug": "Log at the `debug` level.",
              "trace": "Log at the `trace` level.",
              "error": "Log at the `error` level."
            },
            "default": "info"
          },
          {
            "name": "rate_limit_secs",
            "description": "Specifies that the log message is output no more than once per the given number of seconds.\nUse a value of `0` to turn rate limiting off.",
            "required": false,
            "type": [
              "integer"
            ],
            "default": "1"
          }
        ],
        "return": {
          "types": [
            "null"
          ]
        },
        "examples": [
          {
            "title": "Log a message",
            "source": "log(\"Hello, World!\", level: \"info\", rate_limit_secs: 60)",
            "return": null
          },
          {
            "title": "Log an error",
            "source": ". = { \"field\": \"not an integer\" }\n_, err = to_int(.field)\nif err != null {\n    log(err, level: \"error\")\n}\n",
            "return": null
          }
        ],
        "pure": false
      }
    }
  }
}
