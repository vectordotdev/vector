Add a new `opsimate` sink that delivers log events to OpsiMate as alerts. Each event is sent, after the
`encoding` transformer is applied, as a single JSON alert object to the `/api/v1/alerts/custom` endpoint
of the configured `endpoint`, authenticated with the `api_token` option. The sink supports the usual
`batch`, `request`, `tls` and `acknowledgements` options and checks `{endpoint}/health` on startup.

authors: idanlodzki
