The `json` encoding codec has a new `json.bytes` option that controls how byte values are written.
The default, `lossy_utf8`, keeps the current behavior of replacing invalid UTF-8 sequences with
U+FFFD. With `base64`, byte values are written as padded base64 strings using the standard
alphabet, so binary data, such as binary frames received by the `websocket` source, is preserved.
Events store string values as bytes, so `base64` applies to every string value in log and trace
events.

authors: danielku15
