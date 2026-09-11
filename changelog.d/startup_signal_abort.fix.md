Fix an issue where Vector would not respond to shutdown signals (`SIGINT`, `SIGQUIT`,
`SIGTERM`) while it was still starting up or running `vector validate`. If startup was blocked
— for example, on a sink healthcheck or API version probe for an unreachable endpoint — the
signal was logged but ignored until startup completed or failed, which could take minutes.
Shutdown signals received during startup now abort it immediately.

authors: thomasqueirozb
