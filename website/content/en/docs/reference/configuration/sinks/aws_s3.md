---
title: AWS S3
kind: sink
---

## Configuration

{{< component/config >}}

## Environment variables

{{< component/env-vars >}}

## Telemetry

{{< component/config >}}

## How it works

### AWS authentication

{{< snippet "aws/auth" >}}

### Buffers and batches

{{< snippet "buffers-and-batches" >}}

### Cross-account object writing

If you're using Vector to write objects across AWS accounts, you should consider setting the [`grant_full_control`](#grant_full_control) option to the bucket owner's canonical user ID. AWS provides a [full tutorial][tutorial] for this use case. If don't know the bucket owner's canonical ID you can find it by following [this tutorial][canonical_id_tutorial].

### Health checks

{{< snippet "health-checks" >}}

### Object Access Control List (ACL)

AWS S3 supports [access control lists (ACL)][acl] for buckets and objects. In the context of Vector, only object ACLs are relevant (Vector does not create or modify buckets). You can set the object level ACL by using one of the [`acl`](#acl), [`grant_full_control`](#grant_full_control), [`grant_read`](#grant_read), [`grant_read_acp`](#grant_read_acp), or [`grant_write_acp`](#grant_write_acp) options.

#### [`acl.*`](#acl) options

The `grant_`* options name a specific entity to grant access to. The [`acl`](#acl) options are a set of [specific canned ACLs][canned_acls] that can only name the owner or world.

### Object tags and metadata

Vector currently supports [AWS S3 object tags][s3_tags] but *not* [object metadata][s3_metadata]. If you need metadata support, see [issue 1694][issue_1694].

We believe tags are more flexible since they are separate from the actual S3 object. You can freely modify tags without modifying the object. Conversely, object metadata requires a full rewrite of the object to make changes.

### Object naming

By default, Vector names your S3 storage objects in the following format:

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

### Rate limits and adaptive concurrenct

{{< snippet "arc" >}}

### Retry policy

{{< snippet "retry-policy" >}}

### Server-side encryption (SSE)

AWS S3 offers [server-side encryption][sse]. You can apply defaults at the bucket level or set the encryption at the object level. In the context, of Vector only the object level is relevant (Vector does not create or modify buckets). Although, we recommend setting defaults at the bucket level whne possible. You can explicitly set the object level encryption via the [`server_side_encryption`](#server_side_encryption) option.

### State

{{< snippet "stateless" >}}

### Storage class

AWS S3 offers [storage classes][storage_class]. You can apply defaults, and rules, at the bucket level or set the storage class at the object level. In the context of Vector only the object level is relevant (Vector does not create or modify buckets). You can set the storage class via the [`storage_class`](#storage_class) option.

[acl]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html
[canned_acls]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#canned-acl
[canonical_id_tutorial]: https://docs.aws.amazon.com/general/latest/gr/acct-identifiers.html#FindingCanonicalId
[control_tutorial]: https://docs.aws.amazon.com/AmazonS3/latest/dev/example-walkthroughs-managing-access-example3.html
[issue_1694]: https://github.com/timberio/vector/issues/1694
[s3_metadata]: https://docs.aws.amazon.com/AmazonS3/latest/dev/UsingMetadata.html#object-metadata
[s3_tags]: https://docs.aws.amazon.com/AmazonS3/latest/user-guide/add-object-tags.html
[sse]: https://docs.aws.amazon.com/AmazonS3/latest/dev/UsingServerSideEncryption.html
[storage_class]: https://aws.amazon.com/s3/storage-classes/
[uuidv4]: https://en.wikipedia.org/wiki/Universally_unique_identifier#Version_4_(random)
