---
date: "2022-04-06"
title: "Switching the default implementation of disk buffers to `disk_v2`"
description: "After much testing, we're promoting `disk_v2` to stable to bring performance and efficiency benefits to everyone using disk buffers."
authors: ["tobz"]
pr_numbers: [12069]
release: "0.22.0"
hide_on_release_notes: false
badges:
  type: enhancement
---

Back in February, we announced the beta release of our new, reworked disk buffer implementation --
the so-called `disk_v2` buffer -- as part of Vector 0.20.0.  Today, we're excited to announce that
`disk_v2` is now considered stable and is being promoted to the default implementation for `disk`
buffers.

## Wait a minute, what's `disk_v2`?  I use a `disk` buffer

As part of reworking the implementation of disk buffers, we needed to write the new implementation
alongside the existing disk buffers. This was necessary so that we could build confidence in the new
implementation before making it the default, as well to provide us time to write the necessary
documentation and migration procedures and so on.

We called the new implementation `disk_v2` to distinguish it from `disk`. You were able to specify
this in your buffer configuration to opt in to using them when in beta. Now that we're comfortable
marking the new implementation as stable, we've changed their name so that `disk_v2` is now `disk`,
and what used to be `disk` is now `disk_v1`.

## Do I have to do anything to migrate? What happens to my data?

While there were many reasons to write a new implementation of disk buffers -- fewer code
dependencies, more consistent performance, better guarantees around data durability -- we've tried
to keep the user experience foremost in our minds: switching to using `disk_v2` should be as
painless as possible.

When running Vector 0.22.0, if we detect a disk buffer that was created with the old `disk_v1`
implementation, Vector will seamlessly migrate it to the new `disk_v2` format and use the new format
going forward.

This does come with a few caveats:

- Vector needs free space to write into the new buffer as it's migrating the old buffer over
- Vector will delete the old buffer once all records have been migrated

While we do try to maximize buffer compaction during the migration -- basically, delete old data as
it's migrated -- the process is eventually consistent, and migration won't always stay at or below
the configured maximum buffer size.  With this in mind, it is best to plan for having free space
beyond the configured maximum buffer size limit -- in practice, 10 to 15% extra is sufficient -- to
allow the migration to complete successfully.

Additionally, as the migration process is destructive -- the old buffer is migrated and then removed
-- you may wish to make a copy of the buffer data directories (located under the `data_dir` path
specified in your configuration) before running Vector 0.22.0.  This will allow you to roll back to
Vector 0.21 (or earlier) if necessary.

## Let us know what you think!

We're still just as excited about the performance improvements to disk buffers, and have exciting
plans for extending buffering capabilities as a whole.  If you have any feedback for us, whether
it's related to the new disk buffers or anything else, let us know on [Discord] or on [Twitter].

[Discord]: https://discord.com/invite/dX3bdkF
[Twitter]: https://twitter.com/vectordotdev
