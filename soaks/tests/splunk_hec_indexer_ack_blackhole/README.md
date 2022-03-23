# Splunk HEC -> Blackhole

This soak tests the Splunk HEC source feeding directly into the blackhole sink
with acknowledgements enabled. This tests the best case scenario performance of
the indexer acknowledgements logic in the Splunk HEC source.

## Method

Lading `splunk_hec_gen` is used to generate Splunk HEC load into vector.
