# Native event encoding fixtures

This directory contains test fixture data for the native protobuf and JSON
codecs. These fixtures were generated when the feature was first implemented,
and we test that all the examples can be successfully parsed, parse the same
across both formats, and match the current serialized format.

In order to avoid small inherent serialization differences between JSON and
protobuf (e.g. float handling), some changes were made to the `Arbitrary`
implementation for `Event` to give simpler values. These are not changes we want
in most property testing scenarios, but they are appropriate in this case where
we only care about the overall structure of the events. The diff needed is
committed here as `generate_fixtures.patch`. Part of that patch is a `roundtrip`
test definition that writes out the appropriate files, which then need to be
moved to their location here.
