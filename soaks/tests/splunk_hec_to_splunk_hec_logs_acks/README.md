# Splunk HEC -> Splunk HEC Logs

This soak tests a straight pipeline receiving data from a Splunk HEC source and
sending to a Splunk HEC logs sink with no transforms in between and end-to-end
acknowledgements enabled.

## Method

Lading `splunk_hec_gen` is used to generate log load into vector,
`splunk_hec_blackhole` acts as a Splunk HEC sink.
