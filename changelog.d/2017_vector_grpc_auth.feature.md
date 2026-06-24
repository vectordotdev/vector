The `vector` source and sink now support an optional `auth` option for authenticating
requests between Vector instances. It accepts the same `bearer`, `basic`, and `custom`
strategies as the HTTP components, so a token can be supplied through a secrets backend
with `SECRET[...]`. The sink sends the credentials on every request and the source rejects
requests that do not present them. TLS should be enabled so the credentials are not sent in
plaintext.

authors: stigglor
