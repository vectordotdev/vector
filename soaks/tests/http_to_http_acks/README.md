# HTTP -> HTTP

This soak tests the simplest pipeline receiving data from a HTTP source
and sending to a HTTP sink. It is a straight pipe with no transforms or
other work going on, meant to test the best case performance of Vector's
HTTP stack.

## Method

Lading `http_gen` is used to generate log load into vector, `http_blackhole`
acts as a HTTP sink.
