Datadog sink custom endpoints without a scheme now default to `https://`. Query parameters are
removed when Vector appends its Datadog API path; endpoint query parameters are not supported for
configuring Datadog API requests.

When migrating a configuration that relies on endpoint query parameters, remove the query string
and use the Datadog sink's supported request settings or headers instead. Specify `http://` or
`https://` explicitly when the endpoint must use a particular scheme.

authors: kurochan
