The `elasticsearch` sink now logs each individually rejected bulk-API event at debug level, including Elasticsearch error type, reason, per-item status, and the original event.

authors: benmali
