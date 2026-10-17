Add an `opensearch` sink for self-managed OpenSearch, Amazon OpenSearch Service,
and Amazon OpenSearch Serverless. It shares the Elasticsearch sink's configuration
options and delivery behavior. Automatic API version detection now recognizes
OpenSearch in both sinks, so an explicit `api_version: v7` is no longer needed.

authors: pront
