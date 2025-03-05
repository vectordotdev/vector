## Protobuf files for encoding tests

These proto files are used in [src/sinks/util/encoding.rs](../../../src/sinks/util/encoding.rs) tests to confirm framing works as intended

### Regenerate

There is a Makefile to ease the process of compiling the test binary file. It requires `protobuf` to be installed in a python3 environment.

* `make generate-test-payload`
  * this script will generate the required *_pb2.py and serialise a test message.

