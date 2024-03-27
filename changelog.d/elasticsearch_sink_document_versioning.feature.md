Allows the Elasticsearch sink to use [external versioning for documents](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-index_.html#index-versioning). To use it set `bulk.version_type` to `external` and then set `bulk.version` to either some static value like `123` or use templating to use an actual field from the document `{{ my_document_field }}`.

authors: radimsuckr
