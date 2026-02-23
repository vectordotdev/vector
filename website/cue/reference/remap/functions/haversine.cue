{
  "remap": {
    "functions": {
      "haversine": {
        "anchor": "haversine",
        "name": "haversine",
        "category": "Map",
        "description": "Calculates [haversine](https://en.wikipedia.org/wiki/Haversine_formula) distance and bearing between two points.",
        "arguments": [
          {
            "name": "latitude1",
            "description": "Latitude of the first point.",
            "required": true,
            "type": [
              "float"
            ]
          },
          {
            "name": "longitude1",
            "description": "Longitude of the first point.",
            "required": true,
            "type": [
              "float"
            ]
          },
          {
            "name": "latitude2",
            "description": "Latitude of the second point.",
            "required": true,
            "type": [
              "float"
            ]
          },
          {
            "name": "longitude2",
            "description": "Longitude of the second point.",
            "required": true,
            "type": [
              "float"
            ]
          },
          {
            "name": "measurement_unit",
            "description": "Measurement system to use for resulting distance.",
            "required": false,
            "type": [
              "string"
            ],
            "enum": {
              "miles": "Use miles for the resulting distance.",
              "kilometers": "Use kilometers for the resulting distance."
            }
          }
        ],
        "return": {
          "types": [
            "object"
          ]
        },
        "examples": [
          {
            "title": "Haversine in kilometers",
            "source": "haversine(0.0, 0.0, 10.0, 10.0)",
            "return": {
              "bearing": 44.561,
              "distance": 1568.5227233
            }
          },
          {
            "title": "Haversine in miles",
            "source": "haversine(0.0, 0.0, 10.0, 10.0, measurement_unit: \"miles\")",
            "return": {
              "bearing": 44.561,
              "distance": 974.6348468
            }
          }
        ],
        "pure": true
      }
    }
  }
}