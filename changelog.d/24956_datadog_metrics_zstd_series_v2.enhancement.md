The `datadog_metrics` sink now uses zstd compression when submitting metrics to the Series v2 endpoint (`/api/v2/series`). Series v1 and Sketches continue to use zlib (deflate).

authors: vladimir-dd
