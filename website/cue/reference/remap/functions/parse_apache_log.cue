{
  "remap": {
    "functions": {
      "parse_apache_log": {
        "anchor": "parse_apache_log",
        "name": "parse_apache_log",
        "category": "Parse",
        "description": "Parses Apache access and error log lines. Lines can be in [`common`](https://httpd.apache.org/docs/current/logs.html#common),\n[`combined`](https://httpd.apache.org/docs/current/logs.html#combined), or the default [`error`](https://httpd.apache.org/docs/current/logs.html#errorlog) format.",
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
            "name": "format",
            "description": "The format to use for parsing the log.",
            "required": true,
            "type": [
              "string"
            ],
            "enum": {
              "common": "Common format",
              "combined": "Apache combined format",
              "error": "Default Apache error format"
            }
          },
          {
            "name": "timestamp_format",
            "description": "The [date/time format](https://docs.rs/chrono/latest/chrono/format/strftime/index.html) to use for\nencoding the timestamp. The time is parsed in local time if the timestamp does not specify a timezone.",
            "required": false,
            "type": [
              "string"
            ],
            "default": "%d/%b/%Y:%T %z"
          }
        ],
        "return": {
          "types": [
            "object"
          ]
        },
        "internal_failure_reasons": [
          "`value` does not match the specified format.",
          "`timestamp_format` is not a valid format string.",
          "The timestamp in `value` fails to parse using the provided `timestamp_format`."
        ],
        "examples": [
          {
            "title": "Parse using Apache log format (common)",
            "source": "parse_apache_log!(s'127.0.0.1 bob frank [10/Oct/2000:13:55:36 -0700] \"GET /apache_pb.gif HTTP/1.0\" 200 2326', format: \"common\")",
            "return": {
              "host": "127.0.0.1",
              "identity": "bob",
              "message": "GET /apache_pb.gif HTTP/1.0",
              "method": "GET",
              "path": "/apache_pb.gif",
              "protocol": "HTTP/1.0",
              "size": 2326,
              "status": 200,
              "timestamp": "2000-10-10T20:55:36Z",
              "user": "frank"
            }
          },
          {
            "title": "Parse using Apache log format (combined)",
            "source": "parse_apache_log!(\n    s'127.0.0.1 bob frank [10/Oct/2000:13:55:36 -0700] \"GET /apache_pb.gif HTTP/1.0\" 200 2326 \"http://www.seniorinfomediaries.com/vertical/channels/front-end/bandwidth\" \"Mozilla/5.0 (X11; Linux i686; rv:5.0) Gecko/1945-10-12 Firefox/37.0\"',\n    \"combined\",\n)\n",
            "return": {
              "agent": "Mozilla/5.0 (X11; Linux i686; rv:5.0) Gecko/1945-10-12 Firefox/37.0",
              "host": "127.0.0.1",
              "identity": "bob",
              "message": "GET /apache_pb.gif HTTP/1.0",
              "method": "GET",
              "path": "/apache_pb.gif",
              "protocol": "HTTP/1.0",
              "referrer": "http://www.seniorinfomediaries.com/vertical/channels/front-end/bandwidth",
              "size": 2326,
              "status": 200,
              "timestamp": "2000-10-10T20:55:36Z",
              "user": "frank"
            }
          },
          {
            "title": "Parse using Apache log format (error)",
            "source": "parse_apache_log!(\n    s'[01/Mar/2021:12:00:19 +0000] [ab:alert] [pid 4803:tid 3814] [client 147.159.108.175:24259] I will bypass the haptic COM bandwidth, that should matrix the CSS driver!',\n    \"error\"\n)\n",
            "return": {
              "client": "147.159.108.175",
              "message": "I will bypass the haptic COM bandwidth, that should matrix the CSS driver!",
              "module": "ab",
              "pid": 4803,
              "port": 24259,
              "severity": "alert",
              "thread": "3814",
              "timestamp": "2021-03-01T12:00:19Z"
            }
          }
        ],
        "notices": [
          "Missing information in the log message may be indicated by `-`. These fields are omitted in the result."
        ],
        "pure": true
      }
    }
  }
}
