## Fix

Add logic to `buffers::disk::open` which checks for the "old style" vs "new style" buffer data
directories for the given buffer ID, and either renames the old-style path to the new-style path if
the new-style path doesn't already exist, or emits a warning if both exist and there is data left in
the old-style buffer.  If there is no data left, then we rename the old-style directory by appending
`_old` to it, which stops the logic from seeing it the next time, but leaves it around in case
something about the code to read the buffer size is wrong and there _is_ still data left.

## Testing Plan

Start by grabbing Vector binaries for 0.18.1, 0.19.0, and build one from the fix branch.  We'll use
a simple configuration that will read records from stdin and attempt to send them to an HTTP sink.

For the HTTP sink, we'll simply set it to a non-existent endpoint.  Since Vector will retry
"connection refused" errors, and retry them infinitely, the messages will never be acknowledged,
which ensures they remain in the buffer.  For the purposes of verifying that the same data that went
in is still present after renaming the data directory, etc, we can use `netcat` to listen on the
port and inspect the HTTP request made by the sink.

## Test Cases

1. Ensure that the new-style directory stays untouched between 0.19.0 and the fix binary:
    - Run both the 0.19.0 binary and the fix binary with a clean data directory, and ensure they
      both generate the same data directory.
2. Ensure that the old-style buffer data directory gets migrated when there's no new-style buffer
  data directory and the old-style buffer has no data:
    - Run the 0.18.1 binary with a clean data directory and ensure it creates the old-style buffer
      data directory, but don't send any data through.
    - Run the fix binary and ensure that it renames the old-style buffer data directory to the
      new-style buffer data directory.  Again, send no data through.
3. Ensure that the old-style buffer data directory gets migrated when there's no new-style buffer
  data directory and the old-style buffer has data:
    - Run the 0.18.1 binary with a clean data directory and ensure it creates the old-style buffer
      data directory, and send a few records through.
    - Run the fix binary and ensure that it renames the old-style buffer data directory to the
      new-style buffer data directory.
4. Ensure that old-style buffer data directory is left as-is when there is still data in it and the
  new-style buffer data directory exists:
    - Run the fix binary with a clean data directory and ensure that it creates the new-style buffer
      data directory, but don't send any data through.
    - Run the 0.18.1 binary and ensure it creates the old-style buffer data directory, and send a
      few records into it.
    - Run the fix binary and ensure that it leaves the old-style buffer data directory as-is, but
      that it emits a log message, at the warn level, indicating the situation, including the count
      of records still in the old-style buffer.
5. Ensure that old-style buffer data directory is moved to the side when there is no data in it and
  the new-style buffer data directory exists:
    - Run the fix binary with a clean data directory and ensure that it creates the new-style buffer
      data directory, but don't send any data through.
    - Run the 0.18.1 binary and ensure it creates the old-style buffer data directory, but don't
      send any data through.
    - Run the fix binary and ensure that it renames the old-style buffer data directory with the
      `_old` prefix, and that it emits no log message.
