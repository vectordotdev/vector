An error log for the Elasticsearch sink that logs out the response body when errors occur. This was
a log that used to exist in Vector v0.24.0, but was removed in v0.25.0. Some users were depending on
this log to count the number of errors so it was re-added.
