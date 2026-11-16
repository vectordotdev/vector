The `http_client` source now sends `Accept: application/json` when `decoding.codec` is `json`, `native_json` or `gelf` and the default `bytes` framing is used. Previously it sent `Accept: text/plain`.

authors: anpawo
