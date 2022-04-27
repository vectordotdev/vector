# Syslog -> Regex (VRL) -> Log2Metric ->  Datadog Metrics

This soak tests syslog source feeding directly into the Datadog metrics source
via regex parsing and logs to metric transformation. It is a straight pipe.  The
regex step is a touch contrived with the intention of avoiding overhead in the
regex engine and focusing solely on what we discover by going through remap.

## Method

Lading `tcp_gen` is used to generate syslog load into vector, `http_blackhole`
acts as a Datadog API sink.
