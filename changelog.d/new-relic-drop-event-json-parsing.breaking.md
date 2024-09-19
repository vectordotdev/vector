The `new_relic` sink, when sending logs to the `event` API, would try to parse a
field named `message` as JSON and insert the resulting data into the transmitted
event. This undocumented processing has been removed.
