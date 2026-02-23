{
  "remap": {
    "functions": {
      "parse_user_agent": {
        "anchor": "parse_user_agent",
        "name": "parse_user_agent",
        "category": "Parse",
        "description": "Parses the provided `value` as a user agent, which has\n[a loosely defined format](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/User-Agent).\n\nParses on the basis of best effort. Returned schema depends only on the configured `mode`,\nso if the function fails to parse a field it will set it to `null`.",
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
            "name": "mode",
            "description": "Determines performance and reliability characteristics.",
            "required": false,
            "type": [
              "string"
            ],
            "enum": {
              "fast": "Fastest mode but most unreliable. Uses parser from project [Woothee](https://github.com/woothee/woothee).",
              "reliable": "Provides greater reliability than `fast` and retains it's speed in common cases.\nParses with [Woothee](https://github.com/woothee/woothee) parser and with parser from\n[uap project](https://github.com/ua-parser/uap-core) if there are some missing fields\nthat the first parser wasn't able to parse out but the second one maybe can.\n",
              "enriched": "Parses with both parser from [Woothee](https://github.com/woothee/woothee) and parser from\n[uap project](https://github.com/ua-parser/uap-core) and combines results. Result has the full schema.\n"
            },
            "default": "fast"
          }
        ],
        "return": {
          "types": [
            "object"
          ]
        },
        "examples": [
          {
            "title": "Fast mode",
            "source": "parse_user_agent(\n    \"Mozilla Firefox 1.0.1 Mozilla/5.0 (X11; U; Linux i686; de-DE; rv:1.7.6) Gecko/20050223 Firefox/1.0.1\"\n)\n",
            "return": {
              "browser": {
                "family": "Firefox",
                "version": "1.0.1"
              },
              "device": {
                "category": "pc"
              },
              "os": {
                "family": "Linux",
                "version": null
              }
            }
          },
          {
            "title": "Reliable mode",
            "source": "parse_user_agent(\n    \"Mozilla/4.0 (compatible; MSIE 7.66; Windows NT 5.1; SV1; .NET CLR 1.1.4322)\",\n    mode: \"reliable\")\n",
            "return": {
              "browser": {
                "family": "Internet Explorer",
                "version": "7.66"
              },
              "device": {
                "category": "pc"
              },
              "os": {
                "family": "Windows XP",
                "version": "NT 5.1"
              }
            }
          },
          {
            "title": "Enriched mode",
            "source": "parse_user_agent(\n    \"Opera/9.80 (J2ME/MIDP; Opera Mini/4.3.24214; iPhone; CPU iPhone OS 4_2_1 like Mac OS X; AppleWebKit/24.783; U; en) Presto/2.5.25 Version/10.54\",\n    mode: \"enriched\"\n)\n",
            "return": {
              "browser": {
                "family": "Opera Mini",
                "major": "4",
                "minor": "3",
                "patch": "24214",
                "version": "10.54"
              },
              "device": {
                "brand": "Apple",
                "category": "smartphone",
                "family": "iPhone",
                "model": "iPhone"
              },
              "os": {
                "family": "iOS",
                "major": "4",
                "minor": "2",
                "patch": "1",
                "patch_minor": null,
                "version": "4.2.1"
              }
            }
          }
        ],
        "notices": [
          "All values are returned as strings or as null. We recommend manually coercing values\nto desired types as you see fit.",
          "Different modes return different schema.",
          "Field which were not parsed out are set as `null`."
        ],
        "pure": true
      }
    }
  }
}