## Enhancement

Add support to disk buffers to allow for versioning/schema evolution in the future, such that new
versions of Vector can buffer items using different codecs or event schemas, while still potentially
supporting previous versions during deprecation periods.

This primarily changes disk buffer v2 code, which is not yet officially released, so we're cool with
the breaking change there.  This testing focuses on the changes to disk buffer v1, which as designed
should be a no-op, but to avoid data loss and another #10430-esque issue... we want to explicitly
test before merging.

## Testing Plan

Start by grabbing a Vector binary for 0.19.0, and build one from the PR branch.  We'll use
a simple configuration that will read records from stdin and attempt to send them to an HTTP sink.

For the HTTP sink, we'll simply set it to a nonexistent endpoint.  Since Vector will retry
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
   - Start `nc` listening on the relevant port.
   - Run the PR binary and ensure that it reads the records from the buffer and sends them to the
     HTTP sink.
