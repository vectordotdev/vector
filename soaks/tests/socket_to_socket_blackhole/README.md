# Socket -> Socket

This soak tests the socket source feeding directly into the socket sink. No
transformation is done in the middle so throughput is limited solely by
socket source's ability to create it, and socket sink's ability to absorb it.

## Method

Lading `tcp_gen` is used to generate socket load into vector, `tcp_blackhole`
acts as a sink.
