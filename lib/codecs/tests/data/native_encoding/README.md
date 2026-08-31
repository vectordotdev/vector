# Native event encoding fixtures

This directory contains test fixture data for the native protobuf and JSON
codecs. These fixtures were generated when the feature was first implemented,
and we test that all the examples can be successfully parsed, parse the same
across both formats, and match the current serialized format.

These snapshots are intentionally frozen rather than regenerated. New coverage
should use focused historical wire literals or property tests instead of adding
more generated fixture files.
