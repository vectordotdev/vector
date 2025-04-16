Adding an option in the `datadog_logs` sink to allow Vector to mutate the record to conform to the
protocol used by the Datadog Agent itself. To enable use the `conforms_as_agent` option or have the
appropriate agent header (`DD-PROTOCOL: agent-json`) within the additional HTTP Headers list.

The log will be mutated so that any data that is a reserved keyword that exists at the root will be
moved under a new object keyed at the field 'message'. One will be created if it already does not
exist. As an example:

```json
{
  "key1": "value1",
  "key2": { "key2-1" : "value2" },
  "message" : "Hello world",
  ... rest of reserved fields
}
```

will be modified to:

```json
{
  "message" : {
    "message" : "Hello world",
    "key1": "value1",
    "key2": { "key2-1" : "value2" }
  },
  ... rest of reserved fields
}
```

authors: graphcareful
