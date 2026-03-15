Add support for Dead Letter Queue (dlq) on sink and enabling it in Elasticsearch sink.

## Simple use case
Create two indexes, one with mapped filed `value` equal to long and one without mappings like this:

```bash
curl -X PUT "localhost:9200/test-index" -H 'Content-Type: application/json' -d '
{
  "mappings": {
    "properties": {
      "value": {
        "type": "long"
      }
    }
  }
}' -v

curl -X PUT "localhost:9200/test-index-no-mapping" -H 'Content-Type: application/json' -d '{}' -v
```

Then, configure vector like this:

```yaml
api:
  enabled: true
sources:
  my_source:
    type: demo_logs
    format: shuffle
    lines:
      - '{"value":1,"tag":"ok"}'
      - '{"value":2,"tag":"ok"}'
      - '{"value":"bad","tag":"bad"}'
      - '{"value":"bad2","tag":"bad"}'
      - '{"value":3,"tag":"bad"}'

transforms:
  my_transform:
    type: remap
    inputs:
      - my_source
    source: |
      . = parse_json!(.message)
  my_dlq_transform:
    type: remap
    inputs:
      - "es_out.dlq"
    source: |
      .enter_dlq = true
sinks:
  es_out:
    type: elasticsearch
    inputs:
      - my_transform
    endpoints:
      - "http://localhost:9200"
    bulk:
      index: "test-index"
  es_dlq_out:
    type: elasticsearch
    inputs:
      - "my_dlq_transform"
    endpoints:
      - "http://localhost:9200"
    bulk:
      index: "test-index-no-mapping"

```

You should see that the events with `value` field as string will be sent to `test-index-no-mapping` index and the ones with `value` field as long will be sent to `test-index` index.
As per example above, `es_out.dlq` is used as input for `transform` or can be used directly into another `sink`, like filesystem line.

authors: tanganellilore
