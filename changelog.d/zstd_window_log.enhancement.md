Sinks using `zstd` compression accept a `window_log` that sets the match window independently of `level`, for example `compression = { algorithm = "zstd", level = 6, window_log = 27 }`. A larger window can find repeats further apart in large batches, at the cost of more memory to compress and decompress.

authors: IraSkyx
