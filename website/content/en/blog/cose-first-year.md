---
title: Celebrating COSE's First Year
short: COSE Team - First Year Retrospective
description: Celebrating one year of the COSE team and highlighting our contributions to the Vector open source community
authors: [ "pront" ]
date: "2025-11-04"
badges:
  type: retrospective
  domains: [ "dev", "community" ]
tags: [ "community", "open source", "cose", "contributions" ]
---

## Our Journey

One year ago, in October 2024, the
[COSE (Community Open Source Engineering)](https://opensource.datadoghq.com/about/#the-community-open-source-engineering-team)
team was formed with a mission to strengthen Vector's open source foundation and improve the developer experience.
Today, we're celebrating our first year by highlighting the contributions we've made to the Vector community. The
COSE team has committed over **550 changes** to Vector across **7 major releases** (0.43.0 through 0.50.0).

## By the Numbers

Over the past year:

- **550+** commits to Vector
- **7** major releases (0.43.0 through 0.50.0)
- **100+** pull requests merged
- **159** unique community contributors

## What's New for You

### Native OpenTelemetry Support

If you're using OpenTelemetry, Vector now speaks your language natively. You can send OTLP data directly to Vector
and get OTLP data out—no more custom transforms or format conversions. Whether you're collecting logs, metrics, or
traces, Vector now integrates seamlessly into your OpenTelemetry stack as a collector, aggregator, or transformation
layer.

**What this means for you:** Simpler configurations, faster setup, and native compatibility with the OpenTelemetry
ecosystem you're already using.

Read more in our [OTLP Support highlight]({{< ref "/highlights/2025-09-23-otlp-support.md" >}}).

### More Reliable Operations

We've made Vector more robust in production:

- **Automatic TLS certificate rotation**: Your HTTP sinks now reload certificates automatically—no more manual
  restarts when certs expire
- **Smarter configuration reloads**: The `--watch-config` flag now watches external VRL files and enrichment tables
  in addition to config files, automatically reloading when they change. We've also fixed crashes during enrichment
  table reloads and improved file event handling to properly track file changes even during editor save operations
- **More accurate metrics**: Fixed issues with CPU utilization reporting and metric reliability during reloads

**What this means for you:** Fewer surprises at 3am, easier operations, and more confidence in your observability
pipeline.

### Better Documentation

We know good documentation is crucial when you're setting up or troubleshooting Vector:

- **[Debugging Guide]({{< ref "/guides/developer/debugging" >}})**: Comprehensive troubleshooting guide with
  step-by-step instructions, common issues, and debugging techniques
- **[Config Autocompletion Guide]({{< ref "/guides/developer/config-autocompletion" >}})**: Set up your IDE to get
  autocomplete help while writing Vector configs
- **Improved component output documentation**: Clearer explanations of what data types each component produces
- **Reorganized AWS Guides**: Better organized and more comprehensive guides for AWS integrations
- **Enhanced VRL Documentation**: More examples and clearer explanations for VRL functions, including new functions
  like IPCrypt, xxhash, and path manipulation

**What this means for you:** Faster onboarding, easier troubleshooting, and less time hunting for answers.

### More Powerful VRL Functions

We've expanded VRL (Vector Remap Language) with new capabilities for your data transformations:

- **CBOR parsing**: Work with CBOR-encoded data in your pipelines
- **LZ4 compression**: Compress and decompress data on the fly
- **Character set encoding**: Handle different text encodings seamlessly
- **Better duration parsing**: Parse complex durations like `1h2m3s` easily
- **Shannon entropy calculations**: Analyze randomness in your data for security use cases

**What this means for you:** More data transformation options without writing custom code.

### Community Contributions We're Proud Of

We helped bring these community contributions to production (listed alphabetically):

- **Incremental to Absolute Transform** ([@DerekZhang](https://github.com/DerekZhang)): Convert incremental metrics
  to absolute values with intelligent caching
- **Keep Sink** ([@sainad2222](https://github.com/sainad2222)): Send alerts and events to Keep for incident
  management
- **Memory Enrichment Table** ([@esensar](https://github.com/esensar), [@Quad9DNS](https://github.com/Quad9DNS)): Use
  Vector for caching and key-value storage, with per-event TTL
- **MQTT Source** ([@StormStake](https://github.com/StormStake)): Ingest data from MQTT brokers, perfect for IoT
  and edge computing use cases
- **NATS JetStream Support** ([@benjamindornel](https://github.com/benjamindornel)): Full JetStream support for
  reliable NATS messaging
- **Okta Source** ([@JohnSonnenschein](https://github.com/JohnSonnenschein)): Collect security and audit logs
  directly from Okta
- **Postgres Sink** ([@jorgehermo9](https://github.com/jorgehermo9)): Write logs, metrics, and traces directly to
  PostgreSQL databases
- **Redis Sentinel Support** ([@JakeHalaska](https://github.com/JakeHalaska)): High-availability Redis configurations
  now supported
- **Templateable URI for HTTP Sink** ([@jorgehermo9](https://github.com/jorgehermo9)): Dynamically route HTTP requests
  based on event data
- **WebSocket Server Sink** ([@esensar](https://github.com/esensar)): Stream data to WebSocket clients with ACK
  support and buffering
- **WebSocket Source** ([@benjamindornel](https://github.com/benjamindornel)): Stream data from WebSocket
  connections in real-time
- **Window Transform** ([@zvirblis](https://github.com/zvirblis)): Aggregate events over time windows for temporal
  analysis

**What this means for you:** More integration options, new data sources, and flexible routing capabilities to fit
Vector into your existing infrastructure.

## Making Vector Development Better

While our primary focus is making Vector better for you, we've also invested in making it easier for people to
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

## Looking Ahead

As we enter our second year, our focus remains on the themes that have guided us so far:

- **Building a welcoming community**: Making it easier and more rewarding for everyone to contribute to Vector
- **Stability and reliability**: Continuously improving Vector's production-readiness and operational experience
- **Performance improvements**: Finding opportunities to make Vector faster and more efficient
- **Better learning resources**: Expanding documentation and guides to help both new and experienced users
- **Supporting contributors**: Helping community ideas and contributions make their way into Vector

## Thank You

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
- [v0.47.0 Release Notes]({{< ref "/releases/0.47.0" >}}) - April 2025
- [v0.48.0 Release Notes]({{< ref "/releases/0.48.0" >}}) - June 2025
- [v0.49.0 Release Notes]({{< ref "/releases/0.49.0" >}}) - August 2025
- [v0.50.0 Release Notes]({{< ref "/releases/0.50.0" >}}) - September 2025

### VRL Functions Added

For a complete list of VRL functions added during this period, see
the [VRL Changelog](https://github.com/vectordotdev/vrl/blob/main/CHANGELOG.md).
