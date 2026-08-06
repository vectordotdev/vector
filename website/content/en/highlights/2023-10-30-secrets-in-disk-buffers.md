---
date: "2023-10-30"
title: "Secrets in Disk Buffers"
description: ""
authors: ["tobz"]
pr_numbers: [18816]
release: "0.34.0"
hide_on_release_notes: false
badges:
  type: "announcement"
---

Starting with Vector's `0.34.0` release, secrets in events will now be stored in disk buffers. These
secrets are stored **unencrypted**.

## Event secrets

For some source components in Vector, such as the Datadog Agent or Splunk HEC sources, these
components have the ability to store the API key received in requests by Vector in order to re-use
the same API key when sending those events back out to a compatible service. This allows users, for
example, to set up Vector as an aggregator for all of their Datadog Agent processes, reusing the
original API key, or keys, as the events are then forwarded to the Datadog API.

## Disk buffers and event secrets and metadata

Prior to [#18816](https://github.com/vectordotdev/vector/pull/18816), these event secrets (and other
event metadata) were not stored when using disk buffers. This represented a loss of functionality
when users switched from the default in-memory buffers to disk buffers. In order to bring this
functionality up to par, we added support for storing event secrets/metadata when writing events to
disk buffers.

Naturally, event secrets represent sensitive data such as API keys and more. However, Vector
currently stores these event secrets **unencrypted** in disk buffers.

## Do I need to worry about this change?

Firstly, if you're **not** using disk buffers, then there is no change to Vector's behavior and you can
stop reading here.

There are two main scenarios where a configuration might now start storing secrets in disk buffers:

- When you are using a source component which has the ability to store secrets
- When you are using `remap` and adding secrets directly to events

### Source components that can store secrets

Some source components store secrets (specifically, API keys) on an event in order to
facilitate Vector acting similarly to a proxy, using as much of the original request/event data as
possible. Only two sources currently provide such behavior:

- `datadog_agent` source (stores the `DD-API-KEY` header value; **enabled** by default)
- `splunk_hec` source (stores the `Authorization` header value; **disabled** by default)

However, for both of these sinks, this behavior can be disabled by setting `store_api_key` to
`false` for the `datadog_agent` source, or setting `store_hec_token` to `false` for the `splunk_hec`
source.

### Manually-stored secrets using `remap`

When using the `remap` transform, VRL exposes helper functions to set secrets on events. If your
`remap` usage includes setting secrets, then those secrets would also now be in scope for getting
stored in disk buffers.

## Securing disk buffers

As mentioned above, secrets will now be stored in disk buffer data files, and will be
**unencrypted**. The data directory that Vector is configured to use should be locked down as
tightly as possible so that only the user/group that runs the Vector process has read/write
access.

By default on Unix-based platforms, Vector will attempt to set file permissions for the disk buffer
directory/files to only be readable/writeable by the process user, and only readable by the process
group. This does not occur on Windows.

## Future improvements to disk buffers and securely buffering events

This is not the end of the story for storing secrets in disk buffers. We do have tentative plans to
eventually support encrypting secrets in disk buffers, and potentially support encrypting all event
data itself. This work depends on capabilities Vector does not currently have, such as being able to
securely pass a decryption key into the process, and where a long-lived decryption key would live.

These issues need to be tackled first before we can provide a robust encryption solution for disk
buffers.
