Custom authorization strategy is now supported for sources running
HTTP servers (`http_server` source, `prometheus` source, `datadog_agent`, etc.).

Since there are now multiple authorization strategies, if you are using `auth` in any
of these supported components, you now also need to add `strategy: "basic"`, together with
`user` and `password`.

authors: esensar
