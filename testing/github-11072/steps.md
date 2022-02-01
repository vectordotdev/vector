## Enhancement

As this PR introduces `EventArray`, it represents a change to the type flowing through buffers.  In
order to prevent a backwards-incompatible change, this PR adds a change that allows the encoding
scheme to try to decode as either `EventArray` or `Event` in order to be able to load older buffer
files and continue processing them while using `EventArray` going forward.

This test ensures that we can write `Event`s to the buffer, and then read them out when using the
newer code.

## Testing Plan

Start by grabbing a Vector binary for 0.19.0, and build one from the PR branch.  We'll use
a simple configuration that will read records from stdin and attempt to send them to an HTTP sink.

For the HTTP sink, we'll simply set it to a non-existent endpoint.  Since Vector will retry
"connection refused" errors, and retry them infinitely, the messages will never be acknowledged,
which ensures they remain in the buffer.  For the purposes of verifying that the same data that went
in is still present after renaming the data directory, etc, we can use `netcat` (binary is called
`nc`) to listen on the port and inspect the HTTP request made by the sink.

## Test Case(s)

1. Ensure that records written to a v1 disk buffer from a version of Vector without this change can
   be read back from a version of Vector _with_ this change:
   - Start the test without `nc` listening on the HTTP port.
   - Run the 0.19.0 binary with a clean data directory and send a few records through.
   - Stop Vector.
   - Run the PR binary with a clean data directory and send a few records through.
   - Stop Vector.
   - Start `nc` listening on the relevant port.
   - Run the PR binary and ensure that it reads the records from the buffer and sends them to the
     HTTP sink, which should include the records from both the 0.19.0 run and the PR run.
