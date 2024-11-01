# Native event encoding fixtures

This directory contains test fixture data for the native protobuf and JSON
codecs. These fixtures were generated when the feature was first implemented,
and we test that all the examples can be successfully parsed, parse the same
across both formats, and match the current serialized format.

In order to avoid small inherent serialization differences between JSON and
protobuf (e.g. float handling), some changes were made to the `Arbitrary`
implementation for `Event` to give simpler values. These are not changes we want
in most property testing scenarios, but they are appropriate in this case where
we only care about the overall structure of the events.

There is currently a multi-step procedure to re-generate the data files.
There are two diffs committed to this directory:
    - `vector_generate_fixtures.patch`
    - `vrl_generate_fixtures.patch`

The `vrl_` one must be applied to the vectordotdev/vrl repo.
The `vector_` one must be applied to the vector repo (you are here).

Part of the vector patch file is a `roundtrip` unit test definition that needs
to be evoked from `lib/vector-core`. Before invoking it, the `_json` and `_proto`
directories need to be created.

```bash
    $ cd lib/vector-core
    $ mkdir _json/ proto/
    $ cargo test event::test::serialization::roundtrip
```

That test case writes out the appropriate files into the dirs, which then need to be
moved to their location here.
