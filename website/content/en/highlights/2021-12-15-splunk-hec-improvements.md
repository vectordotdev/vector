---
date: "2021-12-15"
title: "Splunk HEC Improvements"
description: "Improved compatibility with Splunk HTTP Event Collector"
authors: ["barieom"]
pr_numbers: [10261, 10135]
release: "0.19.0"
hide_on_release_notes: false
badges:
  type: enhancement
---

We're excited to share that we've made a few improvements to Vector's support
for [Splunk HEC][Splunk HEC]; Vector now supports Splunk HEC indexer
acknowledgements and channel token passthrough routing.

## Indexer Acknowledgements

With the new support for Splunk HEC indexer acknowledgement, Vector is now able
to provide higher guarantees that no data is lost when using the `splunk_hec`
source and sink.

As you may be aware, Splunk HEC does not guarantee that data is successfully
written when it responds to an incoming request successfully. To confirm that data has been persisted, the [indexer acknowledgements][indexer] feature must be enabled. The status of sent events can then be queried via a separate endpoint.

### `splunk_hec` source acknowledgements

Previously, the `splunk_hec`  source did not support [indexer
acknowledgements][indexer] and so would provide weaker delivery guarantees and
would not work with Splunk senders that required them.

Now, you can configure the `splunk_hec` source to use the indexer acknowledgements protocol by configuring:

```toml
acknowledgements = true
```

When enabled, responses to incoming requests will include an ID that can be used to query for acknowledgement status at the newly exposed `/services/collector/ack` endpoint ([learn more here][indexer how it works]). The acknowledgement status is wired into Vector's
forthcoming end-to-end acknowledgement feature. This feature will require that
events be sent by sinks or persisted into disk buffers before sources will
acknowledge them.

### `splunk_hec` sink acknowledgements

Previously, the `splunk_hec` sink simply treated successful HEC requests as the
events being acknowledged by the Spunk receiver and so Vector would drop them
from any buffers. Now, it is possible to configure the sink to wait until the
Splunk receiver acknowledges the events via the [index
acknowledgements][indexer] part of the HEC protocol.

This has defaulted to on to provide higher guarantees, but can be disabled to
restore the previous behavior by configuring:

```toml
acknowledgements.indexer_acknowledgements_enabled = false
```

## Passthrough Token Routing

Vector now also supports Splunk HEC token passthrough routing. When
`store_hec_token` is enabled in a `splunk_hec` source, tokens included in
requests to the source will be stored and used by downstream `splunk_hec` sinks.
Any passed through token takes precedence over the `default_token` configuration
set in the sink.

We hope that these improvements can make your experience using Vector with
Splunk better! If you any feedback for us, let us know on our [Discord chat] or
on [Twitter].

[Splunk HEC]: https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/UsetheHTTPEventCollector
[indexer]: https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/AboutHECIDXAck
[indexer how it works]: https://master.vector.dev/docs/reference/configuration/sinks/splunk_hec_metrics/#indexer-acknowledgements
[Discord chat]: https://discord.com/invite/dX3bdkF
[Twitter]: https://twitter.com/vectordotdev
