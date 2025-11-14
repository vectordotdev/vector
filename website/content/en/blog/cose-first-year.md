---
title: Celebrating COSE's First Year
short: COSE Team - First Year Retrospective
description: Celebrating the first year of the COSE team and highlighting our contributions to the Vector open source community
authors: [ "pront" ]
date: "2025-11-13"
badges:
  type: retrospective
  domains: [ "dev", "community" ]
tags: [ "community", "open source", "cose", "contributions" ]
---

## Our journey

In October 2024, the
[COSE (Community Open Source Engineering)](https://opensource.datadoghq.com/about/#the-community-open-source-engineering-team)
team was formed with the mission to strengthen Vector's open source foundation and improve the developer experience.
Today, we're celebrating our first year by highlighting the contributions we've made to the Vector community. The
COSE team has committed over **550 changes** to Vector, across **8 major releases** (0.43.0 through 0.51.0).

## By the numbers

Over the past year, there were:

- **550+** commits made to Vector
- **8** major releases (0.43.0 through 0.51.0)
- **100+** pull requests merged
- **159** unique community contributors

## What's new for you

### Native OpenTelemetry support

If you're using OpenTelemetry, you can send OTLP data directly to Vector
and get OTLP data out, without needing custom transforms or format conversions. Whether you're collecting logs,
metrics, or traces, Vector now integrates seamlessly into your OpenTelemetry stack as a collector, aggregator, or
transformation layer.

**What this means for you:** Simpler configurations, faster setup, and native compatibility with the OpenTelemetry
ecosystem you're already using.

Read more in our [OTLP Support highlight]({{< ref "/highlights/2025-09-23-otlp-support.md" >}}).

### More reliable operations

We've made Vector more robust in production:

- **Smarter configuration reloads**: The `--watch-config` flag now watches external VRL files and enrichment tables
  in addition to configuration files, automatically reloading when the file or table has been updated. We've also fixed crashes during enrichment
  table reloads and improved file event handling to properly track file changes, even during save operations in the editor.
- **More accurate metrics**: Fixed issues with CPU utilization reporting and metric reliability during reloads.

**What this means for you:** Easier to manage and have more confidence in your observability
pipeline.

### Better documentation

We know good documentation is crucial when you're setting up or troubleshooting Vector, so the following documentation has been added:

- **[Debugging Guide]({{< ref "/guides/developer/debugging" >}})**: Comprehensive troubleshooting guide with
  step-by-step instructions, common issues, and debugging techniques
- **[Config Autocompletion Guide]({{< ref "/guides/developer/config-autocompletion" >}})**: Set up your IDE to get
  autocomplete help while writing Vector configurations
- **Improved component output documentation**: Clearer explanations of what data types each component produces
- **Reorganized AWS Guides**: Better organized and more comprehensive guides for AWS integrations
- **Enhanced VRL Documentation**: More examples and clearer explanations for VRL functions, including new functions
  like IPCrypt, xxhash, and path manipulation

**What this means for you:** Faster onboarding, easier troubleshooting, and less time hunting for answers.

### More powerful VRL functions

We've expanded VRL (Vector Remap Language) with new capabilities for your data transformations:

- **CBOR parsing**: Work with CBOR-encoded data in your pipelines
- **LZ4 compression**: Compress and decompress data on the fly
- **Character set encoding**: Handle different text encodings seamlessly
- **Better duration parsing**: Parse complex durations like `1h2m3s` easily
- **Shannon entropy calculations**: Analyze randomness in your data for security use cases

**What this means for you:** More data transformation options without having to write custom code.

### We're proud of these community contributions

We helped bring these community contributions to production (listed alphabetically):

- **Incremental to absolute transform** ([@DerekZhang](https://github.com/DerekZhang)): Convert incremental metrics
  to absolute values with intelligent caching
- **Keep sink** ([@sainad2222](https://github.com/sainad2222)): Send alerts and events to the Keep sink for incident
  management
- **Memory enrichment table** ([@esensar](https://github.com/esensar), [@Quad9DNS](https://github.com/Quad9DNS)): Use
  Vector for caching and key-value storage, with per-event TTL
- **MQTT source** ([@StormStake](https://github.com/StormStake)): Ingest data from MQTT brokers, helpful for IoT
  and edge computing use cases
- **NATS JetStream support** ([@benjamindornel](https://github.com/benjamindornel)): Full JetStream support for
  reliable NATS messaging
- **Okta source** ([@JohnSonnenschein](https://github.com/JohnSonnenschein)): Collect security and audit logs
  directly from Okta
- **Postgres sink** ([@jorgehermo9](https://github.com/jorgehermo9)): Write logs, metrics, and traces directly to
  PostgreSQL databases
- **Redis Sentinel support** ([@JakeHalaska](https://github.com/JakeHalaska)): High-availability Redis configurations
  now supported
- **Template URI for HTTP sink** ([@jorgehermo9](https://github.com/jorgehermo9)): Dynamically route HTTP requests
  based on event data
- **WebSocket Server sink** ([@esensar](https://github.com/esensar)): Send data to WebSocket clients with ACK
  support and buffering
- **WebSocket source** ([@benjamindornel](https://github.com/benjamindornel)): Send data from WebSocket
  connections in real time
- **Window transform** ([@zvirblis](https://github.com/zvirblis)): Aggregate events over time windows for temporal
  analysis

**What this means for you:** More integration options, new data sources, and flexible routing capabilities to fit
Vector into your existing infrastructure.

## Improving Vector development

While our primary focus is making Vector better for you, we've also invested in making it easier for you to
contribute to Vector. Despite being a small team, we've worked hard to review pull requests quickly and provide
thoughtful feedback to encourage community development. We believe a healthy contributor community means better
software for everyone.

This benefits you through:

- **Faster releases**: Contributors get feedback 60% faster, meaning features and fixes reach you sooner
- **Higher quality**: Better testing infrastructure and thorough code review means fewer bugs in releases
- **More contributors**: When contributing is easier and more welcoming, more people can help improve Vector

_Want to contribute to Vector? Check out our:_

- _[Contribution Guide](https://github.com/vectordotdev/vector/blob/master/CONTRIBUTING.md)_
- _[Debugging Guide]({{< ref "/guides/developer/debugging" >}})_
- _[Support page]({{< ref "/community" >}})_

## Looking ahead

As we enter our second year, our focus remains on the themes that have guided us so far:

- **Building a welcoming community**: Making it easier and more rewarding for everyone to contribute to Vector
- **Stability and reliability**: Continuously improving Vector's production readiness and operational experience
- **Performance improvements**: Finding opportunities to make Vector faster and more efficient
- **Better learning resources**: Expanding documentation and guides to help both new and experienced users
- **Supporting contributors**: Helping community ideas and contributions make their way into Vector

## Thank you

Thank you to the Vector community for your support, feedback, and contributions. The open source community is what
makes Vector great, and we're honored to be part of it.

Here's to another year of building great open source software together! 🚀

---

## Appendix

For those interested in the technical details:

### Release Notes

- [v0.43.0 Release Notes]({{< ref "/releases/0.43.0" >}}) - November 2024
- [v0.44.0 Release Notes]({{< ref "/releases/0.44.0" >}}) - January 2025
- [v0.45.0 Release Notes]({{< ref "/releases/0.45.0" >}}) - February 2025
- [v0.46.0 Release Notes]({{< ref "/releases/0.46.0" >}}) - April 2025
- [v0.47.0 Release Notes]({{< ref "/releases/0.47.0" >}}) - May 2025
- [v0.48.0 Release Notes]({{< ref "/releases/0.48.0" >}}) - June 2025
- [v0.49.0 Release Notes]({{< ref "/releases/0.49.0" >}}) - August 2025
- [v0.50.0 Release Notes]({{< ref "/releases/0.50.0" >}}) - September 2025
- [v0.51.0 Release Notes]({{< ref "/releases/0.51.0" >}}) - November 2025

### VRL functions added

For a complete list of VRL functions added during this period, see
the [VRL Changelog](https://github.com/vectordotdev/vrl/blob/main/CHANGELOG.md).
