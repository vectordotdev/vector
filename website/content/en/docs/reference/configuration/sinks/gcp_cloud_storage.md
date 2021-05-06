---
title: GCP Cloud Storage (GCS)
short: GCP Cloud Storage
kind: sink
---

## Configuration

{{< component/config >}}

## Environment variables

{{< component/env-vars >}}

## Telemetry

{{< component/config >}}

## How it works

### Buffers and batches

{{< snippet "buffers-and-batches" >}}

### GCP authentication

{{< snippet "gcp/auth" >}}

### Health checks

{{< snippet "health-checks" >}}

### Object access control lists (ACL)

GCP Cloud Storage supports access control lists (ACL) for buckets and objects. In the context of Vector, only object ACLs are relevant (Vector does not create or modify buckets). You can set the object level ACL by using the [`acl`](#acl) option, which allows you to set one of the [predefined ACLs][acls] on each created object.

### Object naming

By default, Vector names your GCP storage objects in the following format:

{{< tabs default="Without compression" >}}
{{< tab title="Without compression" >}}
```
<key_prefix><timestamp>-<uuidv4>.log
```

For example:

```
date=2019-06-18/1560886634-fddd7a0e-fad9-4f7e-9bce-00ae5debc563.log
```
{{< /tab >}}
{{< tab title="With compression" >}}
```
<key_prefix><timestamp>-<uuidv4>.log.gz
```

For example:

```
date=2019-06-18/1560886634-fddd7a0e-fad9-4f7e-9bce-00ae5debc563.log.gz
```
{{< /tab >}}
{{< /tabs >}}

Vector appends a [UUIDv4] token to ensure that there are no name conflicts in the unlikely event that two Vector instances write data at the same time.

You can control the resulting name via the [`key_prefix`](#key_prefix), [`filename_time_format`](#filename_time_format), and [`filename_append_uuid`](#filename_append_uuid) options.

### Partitioning

{{< snippet "partitioning" >}}

### Rate limits and adaptive concurrency

{{< snippet "arc" >}}

### Retry policy

{{< snippet "retry-policy" >}}

### State

{{< snippet "stateless" >}}

### Storage class

GCS offers [storage classes][classes]. You can apply defaults, and rules, at the bucket level or set the storage class at the object level. In the context of Vector only the object level is relevant (Vector does not create or modify buckets). You can set the storage class via the [`storage_class`](#storage_class) option.

### Tags and metadata

Vector supports adding [custom metadata][metadata] to created objects. These metadata items are a way of associating extra data items with the object that are not part of the uploaded data.

### Transport Layer Security (TLS)

{{< snippet "tls" >}}

[acls]: https://cloud.google.com/storage/docs/access-control/lists#predefined-acl
[classes]: https://cloud.google.com/storage/docs/storage-classes
[metadata]: https://cloud.google.com/storage/docs/metadata#custom-metadata
