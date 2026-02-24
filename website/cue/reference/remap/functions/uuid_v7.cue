{
  "remap": {
    "functions": {
      "uuid_v7": {
        "anchor": "uuid_v7",
        "name": "uuid_v7",
        "category": "Random",
        "description": "Generates a random [UUIDv7](https://datatracker.ietf.org/doc/html/draft-peabody-dispatch-new-uuid-format-04#name-uuid-version-7) string.",
        "arguments": [
          {
            "name": "timestamp",
            "description": "The timestamp used to generate the UUIDv7.",
            "required": false,
            "type": [
              "timestamp"
            ],
            "default": "`now()`"
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "examples": [
          {
            "title": "Create a UUIDv7 with implicit `now()`",
            "source": "uuid_v7()",
            "return": "0135ddb4-a444-794c-a7a2-088f260104c0"
          },
          {
            "title": "Create a UUIDv7 with explicit `now()`",
            "source": "uuid_v7(now())",
            "return": "0135ddb4-a444-794c-a7a2-088f260104c0"
          },
          {
            "title": "Create a UUIDv7 with custom timestamp",
            "source": "uuid_v7(t'2020-12-30T22:20:53.824727Z')",
            "return": "0176b5bd-5d19-794c-a7a2-088f260104c0"
          }
        ],
        "pure": true
      }
    }
  }
}
