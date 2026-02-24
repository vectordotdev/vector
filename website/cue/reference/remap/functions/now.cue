{
  "remap": {
    "functions": {
      "now": {
        "anchor": "now",
        "name": "now",
        "category": "Timestamp",
        "description": "Returns the current timestamp in the UTC timezone with nanosecond precision.",
        "arguments": [],
        "return": {
          "types": [
            "timestamp"
          ]
        },
        "examples": [
          {
            "title": "Generate a current timestamp",
            "source": "now()",
            "return": "2012-03-04T12:34:56.789012345Z"
          }
        ],
        "pure": true
      }
    }
  }
}
