Disk buffers no longer drop a whole batch of events when the batch is larger than the maximum record size (128 MiB). Vector now splits such a batch into several records, so only an event that is too large on its own is dropped. Previously, a few large log lines arriving together could cause every event in the same batch to be discarded with `Record cannot be written to the disk buffer; dropping it.`

authors: Y--
