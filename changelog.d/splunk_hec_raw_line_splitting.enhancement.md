Add `raw_line_splitting` option to `splunk_hec` source to split incoming requests to the `/services/collector/raw` endpoint on newlines, creating separate events for each line. This is useful when receiving newline-delimited JSON (NDJSON) from sources like CloudFlare Logpush.

