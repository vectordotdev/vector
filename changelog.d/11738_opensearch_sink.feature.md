Add an `opensearch` sink for self-managed OpenSearch, Amazon OpenSearch Service,
and Amazon OpenSearch Serverless. It shares the Elasticsearch sink's configuration
options and delivery behavior. The new sink selects an OpenSearch-compatible bulk
API version by default, so an explicit `api_version: v7` is not needed.

authors: pront
