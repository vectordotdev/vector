The `socket` source in UDP mode now supports a `receive_concurrency` option to run multiple concurrent socket readers. A single reader processes datagrams sequentially, capping receive throughput at one CPU core per socket; multiple readers decode and forward datagrams in parallel. The default of `1` preserves existing behavior.

authors: yackushevas
