The `datadog_metrics` sink now uses zstd compression when submitting metrics to the Series v2 (`/api/v2/series`) and Sketches endpoints. Series v1 continues to use zlib (deflate).

authors: vladimir-dd
