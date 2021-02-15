# Fix Me

This directory contains tests that show a bug in VRL that needs to be resolved.

The first line in these tests should start with:

```ruby
# SKIP
```

This allows the test harness to skip the tests (but still list them as being
skipped).

The second line should provide a link to the open relevant tracking issue:

```text
issue: <link to issue>
```

Usually the file name of the test should be similar to the title of the linked
issue, shortened for brevity.

Once an issue has been resolved, the test should be moved to `tests/issues` and
the `# SKIP` line should be removed. This ensures we keep a trail to the
original issue, and our test-harness captures any regressions in the future.
