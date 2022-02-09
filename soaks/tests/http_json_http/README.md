# HTTP (JSON) -> HTTP

This soak tests a simple pipeline receiving JSON data from a HTTP source and
sending to a HTTP sink. It doesn't do any transformation other than JSON
decoding, meant to test the best case performance of Vector's HTTP stack for
JSON ingestion.

## Method

Lading `http_gen` is used to generate log load into vector, `http_blackhole`
acts as a HTTP sink.
