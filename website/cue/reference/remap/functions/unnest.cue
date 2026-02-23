{
  "remap": {
    "functions": {
      "unnest": {
        "anchor": "unnest",
        "name": "unnest",
        "category": "Object",
        "description": "Unnest an array field from an object to create an array of objects using that field; keeping all other fields.\n\nAssigning the array result of this to `.` results in multiple events being emitted from `remap`. See the\n[`remap` transform docs](/docs/reference/configuration/transforms/remap/#emitting-multiple-log-events) for more details.\n\nThis is also referred to as `explode` in some languages.",
        "arguments": [
          {
            "name": "path",
            "description": "The path of the field to unnest.",
            "required": true,
            "type": [
              "array"
            ]
          }
        ],
        "return": {
          "types": [
            "array"
          ],
          "rules": [
            "Returns an array of objects that matches the original object, but each with the specified path replaced with a single element from the original path."
          ]
        },
        "internal_failure_reasons": [
          "The field path referred to is not an array."
        ],
        "examples": [
          {
            "title": "Unnest an array field",
            "source": ". = {\"hostname\": \"localhost\", \"messages\": [\"message 1\", \"message 2\"]}\n. = unnest(.messages)\n",
            "return": [
              {
                "hostname": "localhost",
                "messages": "message 1"
              },
              {
                "hostname": "localhost",
                "messages": "message 2"
              }
            ]
          },
          {
            "title": "Unnest a nested array field",
            "source": ". = {\"hostname\": \"localhost\", \"event\": {\"messages\": [\"message 1\", \"message 2\"]}}\n. = unnest(.event.messages)\n",
            "return": [
              {
                "event": {
                  "messages": "message 1"
                },
                "hostname": "localhost"
              },
              {
                "event": {
                  "messages": "message 2"
                },
                "hostname": "localhost"
              }
            ]
          }
        ],
        "pure": true
      }
    }
  }
}