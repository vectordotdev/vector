The `logstash` source now rejects compressed frames that are nested inside other compressed frames. Previously, a malicious sender could nest compressed (`C`) frames arbitrarily deep, driving unbounded recursion in the decoder until the process exhausted its stack and aborted. Compressed payloads may now contain only a single layer of compression; no known Lumberjack/Beats client (e.g. Filebeat) ever emits more than one.

authors: thomasqueirozb
