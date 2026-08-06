---
date: "2020-07-13"
title: "ACL is now optional for the Google Cloud Storage sink"
description: "ACL isn't always required when creating objects in GCP Cloud Storage"
authors: ["binarylogic"]
hide_on_release_notes: false
pr_numbers: [2283]
release: "0.9.0"
badges:
  type: "breaking change"
  domain: ["sinks"]
  sinks: ["gcp_cloud_storage"]
---

GCP Cloud Storage buckets with [uniform bucket-level access](https://cloud.google.com/storage/docs/uniform-bucket-level-access)
don't support setting ACL for objects inside them (HTTP 400 error is returned).
Therefore, the Vector `gcp_cloud_storage` sink no longer supplies a
`x-goog-acl` header by default.

## Upgrade Guide

If you wish to set an ACL for your GCP object you'll need to explicitly set
the `acl` option:

```diff title="vector.toml"
 [sinks.gcp_cloud_storage]
   type = "gcp_cloud_storage"
+  acl = "projectPrivate" # change as desired
```

That's it!
