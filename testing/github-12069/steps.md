# Testing Plan - automatically migrate disk v1 buffers to disk v2 (#12069)

## Context

As this PR introduces the change that moves the default `disk` buffer implementation from disk v1 to
disk v2, we similarly add the logic to automatically migrate users' disk buffers from v1 to v2 when
Vector starts.

We do so to ensure that users do not upgrade Vector and have their old buffers fall by the wayside
unbeknownst to them, as it is likely they may unintentionally upgrade such that they do not see any
warning/error logs that we emit to indicate that they must first drain their old buffer before
upgrading, and so on.

Additionally, we did not change the on-disk format for `disk_v2` between 0.20.0 and now, so we
should also be able to create a `disk_v2` buffer with 0.20.0 and continue using it as a `disk`
buffer with a binary built from this PR.

## Plan

Start by grabbing a Vector binary for 0.20.0, and build one from the PR branch.  We'll use a simple
configuration that will read records from stdin and attempt to send them to an HTTP sink.

For the HTTP sink, we'll have one configuration that uses an invalid port and another, an identical
version, which has the correct port. Since Vector will retry "connection refused" errors, and retry
them infinitely, the messages will never be acknowledged, which ensures they remain in the buffer.
For the purposes of verifying that the same data that went in is still present after migrating the
disk v1 buffer, etc, we can use `dummyhttp` (a Rust project for serving up a simplistic HTTP
endpoint that can be configured to respond a certain way) to listen on the port and inspect the HTTP
request made by the sink.

For generating logs so we can fill up our disk buffers to their limit, we'll use [`flog`][flog]
which is a handy tool for generating fake logs, and a lot of them very quickly.

## Test Case(s)

1. Ensure that records written to a v1 disk buffer from a version of Vector without this change can
   be read back after being migrated:
   - Start `dummyhttp` listening on the relevant port.
   - Run the 0.20.0 binary, using the "wrong" configuration, with a clean data directory. The
     `five-lines-first` file should be piped to STDIN.
   - Stop Vector.
   - Run the PR binary, using the "wrong" configuration. The `five-lines-second` file should be
     piped to STDIN.
   - Observe that the old buffer has been migrated, and that five records have been migrated: the
     entries from `five-lines-first`.
   - Stop Vector.
   - Run the PR binary, using the "right" configuration, and ensure that it reads the records from
     the buffer and sends them to the HTTP sink. The `five-lines-second` file should be piped to
     STDIN.

     Overall, there should be fifteen records: all five from the run using Vector 0.20.0, five from
     the run of the PR binary with the wrong configuration, and five from this run.

2. Ensure that records written to a v2 disk buffer from a version of Vector without this change can
   be read back after being migrated:
   - Start `dummyhttp` listening on the relevant port.
   - Run the 0.20.0 binary, using the "wrong disk v2" configuration, with a clean data directory.
     The `five-lines-first` file should be piped to STDIN.
   - Stop Vector.
   - Run the PR binary, using the "wrong" configuration. The `five-lines-second` file should be
     piped to STDIN.
   - Stop Vector.
   - Run the PR binary, using the "right" configuration, and ensure that it reads the records from
     the buffer and sends them to the HTTP sink. The `five-lines-second` file should be piped to
     STDIN.

     Overall, there should be fifteen records: all five from the run using Vector 0.20.0, five from
     the run of the PR binary with the wrong configuration, and five from this run.

3. Ensure that records written to a v1 disk buffer from a version of Vector without this change can
   be migrated over when the new v2 buffer ends up exceeding the configured maximum buffer size:
   - Run the 0.20.0 binary, using the "wrong big buffer" configuration, with a clean data directory.
     `flog` should be piped to STDIN.
   - Stop Vector once the maximum buffer size has been reached.
   - Observe that the old buffer data directory is the only one that exists.
   - Run the PR binary, using the "wrong big buffer" configuration. Do not feed any input to STDIN.
   - Vector should immediately exit after reporting that the old buffer has been migrated.
   - Observe that the old buffer directory is now gone, and the new one should exist.  Likewise, the
     new one should be the same size or larger than the old one.

[flog]: https://github.com/mingrammer/flog
