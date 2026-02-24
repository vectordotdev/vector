{
  "remap": {
    "functions": {
      "http_request": {
        "anchor": "http_request",
        "name": "http_request",
        "category": "System",
        "description": "Makes an HTTP request to the specified URL.",
        "arguments": [
          {
            "name": "url",
            "description": "The URL to make the HTTP request to.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "method",
            "description": "The HTTP method to use (e.g., GET, POST, PUT, DELETE). Defaults to GET.",
            "required": false,
            "type": [
              "string"
            ],
            "default": "get"
          },
          {
            "name": "headers",
            "description": "An object containing HTTP headers to send with the request.",
            "required": false,
            "type": [
              "object"
            ],
            "default": "{  }"
          },
          {
            "name": "body",
            "description": "The request body content to send.",
            "required": false,
            "type": [
              "string"
            ],
            "default": ""
          },
          {
            "name": "http_proxy",
            "description": "HTTP proxy URL to use for the request.",
            "required": false,
            "type": [
              "string"
            ]
          },
          {
            "name": "https_proxy",
            "description": "HTTPS proxy URL to use for the request.",
            "required": false,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "examples": [
          {
            "title": "Basic HTTP request",
            "source": "http_request(\"https://httpbin.org/get\")",
            "return": {
              "args": {},
              "headers": {
                "Accept": "*/*",
                "Host": "httpbin.org"
              },
              "url": "https://httpbin.org/get"
            }
          },
          {
            "title": "HTTP request with bearer token",
            "source": "http_request(\"https://httpbin.org/bearer\", headers: {\"Authorization\": \"Bearer my_token\"})",
            "return": {
              "authenticated": true,
              "token": "my_token"
            }
          },
          {
            "title": "HTTP PUT request",
            "source": "http_request(\"https://httpbin.org/put\", method: \"put\")",
            "return": {
              "args": {},
              "data": "",
              "url": "https://httpbin.org/put"
            }
          },
          {
            "title": "HTTP POST request with body",
            "source": "http_request(\"https://httpbin.org/post\", method: \"post\", body: \"{\\\"data\\\":{\\\"hello\\\":\\\"world\\\"}}\")",
            "return": {
              "data": "{\"data\":{\"hello\":\"world\"}}"
            }
          }
        ],
        "notices": [
          "This function performs synchronous blocking operations and is not recommended for\nfrequent or performance-critical workflows due to potential network-related delays."
        ],
        "pure": true
      }
    }
  }
}
