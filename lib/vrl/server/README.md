# The Vector Remap Language web server

## Run locally

```bash
cargo run
```

By default, the server runs on port 8080. To specify a port:

```bash
cargo run -- --port 1111
```

## Endpoints

There are two server endpoints:

1. `GET /functions` returns a JSON list of the available VRL functions
1. `POST /resolve` takes a VRL `event` (JSON) and a VRL `program` (string) and returns the result as JSON

Here's an example resolution:

```bash
curl -XPOST http://localhost:8080/resolve \
-H "Content-Type: application/json" \
-d '{"program":"del(.foo); .booper = \"bopper\"","event":{"foo": "bar"}}'
```

That should return this result:

```json
{
  "success": {
    "output": "bopper",
    "result": {
      "booper": "bopper"
    }
  }
}
```
