# Testing Plan - Send arrays of events through the topology (#11072)

## Context

As this PR introduces `EventArray`, it represents a change to the type flowing through buffers.  In
order to prevent a backwards-incompatible change, this PR adds a change that allows the encoding
scheme to try to decode as either `EventArray` or `Event` in order to be able to load older buffer
files and continue processing them while using `EventArray` going forward.

This test ensures that we can write `Event`s to the buffer, and then read them out when using the
newer code.

## Plan

Start by grabbing a Vector binary for 0.19.0, and build one from the PR branch.  We'll use a simple
configuration that will read records from stdin and attempt to send them to an HTTP sink.

For the HTTP sink, we'll have one configuration that uses an invalid port and another, an identical
version, which has the correct port. Since Vector will retry "connection refused" errors, and retry
them infinitely, the messages will never be acknowledged, which ensures they remain in the buffer.
For the purposes of verifying that the same data that went in is still present after renaming the
data directory, etc, we can use `dummyhttp` (a Rust project for serving up a simplistic HTTP
endpoint that can be configured to respond a certain way) to listen on the port and inspect the HTTP
request made by the sink.

## Test Case(s)

1. Ensure that records written to a v1 disk buffer from a version of Vector without this change can
   be read back from a version of Vector _with_ this change:
   - Start `dummyhttp` listening on the relevant port.
   - Run the 0.19.0 binary, using the "wrong" configuration, with a clean data directory. The
     `five-lines-first` file should be piped to STDIN.
   - Stop Vector.
   - Run the PR binary, using the "right" configuration, and ensure that it reads the records from
     the buffer and sends them to the HTTP sink.  This should be five records: all five from the run
     using Vector 0.19.0.

2. Ensure that records written to a v1 disk buffer from a version of Vector without this change can
   be read back from a version of Vector _with_ this change, even after additionally writing events
   to the buffer that come from a version of Vector _with_ this change:
   - Start `dummyhttp` listening on the relevant port.
   - Run the 0.19.0 binary, using the "wrong" configuration, with a clean data directory. The
     `five-lines-first` file should be piped to STDIN.
   - Stop Vector.
   - Run the PR binary, using the "wrong" configuration. The `five-lines-second` file should be
     piped to STDIN.
   - Stop Vector.
   - Run the PR binary, using the "right" configuration, and ensure that it reads all the records
     from the buffer and sends them to the HTTP sink.  This should be ten records: all five from
     `five-lines-first`, and all five from `five-lines-second`.
