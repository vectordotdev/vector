Fix. The `axiom` sink has been rebased from `elasticsearch` to `http`.

The elasticsearch `@timestamp` semantics that changed in v5.5 of ElasticSearch no
longer affect the `axiom` sink as the sink uses the axiom native HTTP ingest method:

  https://axiom.co/docs/send-data/ingest#ingest-api

The `_time` field in data sent to axiom now supports the same semantics as the Axiom
native API and SDK as documented here:

  https://axiom.co/docs/reference/field-restrictions#requirements-of-the-timestamp-field

In previous versions of vector, the axiom sink rejected events with `_time` fields as
the sink was following `elasticsearch` semantics. This was confusing and suprising for
seasoned axiom users new to vector and seasoned vector users new to axiom like.

If a `@timestamp` field is sent to Axiom it is a normal user defined field.

If an `_time` field is sent to Axiom it now follows documented Axiom field semantics.

Axiom will no longer reject data from vector with `_time` fields as the sink no
longer users elasticsearch _bulk API endpoint and field semantics with respect to
timestamps.

As the `axiom` sink now uses the native Axiom ingest endpoint the full wealth of
vector's HTTP support is now open to the `axiom` sink where and when needed.

authors: darach
