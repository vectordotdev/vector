The GELF decoder's `relaxed` validation mode now accepts the spec'd numeric fields (`level`,
`line`, and `timestamp`) sent as JSON strings holding numeric values (e.g. `"level": "6"`)
and coerces them to numbers, matching the lenient decoding of the Graylog server. Strict
validation mode is unchanged and still rejects such messages.

authors: yackushevas
