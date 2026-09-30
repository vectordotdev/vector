The `json` codec has a new `encoding.json.bytes` option. Set it to `base64` to write every string
value in log and trace events, including fields such as `host` and `source_type`, as padded
standard base64, so binary data such as frames received by the `websocket` source is preserved.
The default, `lossy_utf8`, keeps the current behavior of replacing invalid UTF-8 sequences with
U+FFFD.

authors: danielku15
