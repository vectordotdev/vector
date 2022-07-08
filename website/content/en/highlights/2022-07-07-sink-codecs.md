---
date: "2021-07-07"
title: "New `encoding` and `framing` options for sinks"
description: ""
authors: ["jszwedko"]
pr_numbers: []
release: "0.23.0"
hide_on_release_notes: false
badges:
  type: "announcement"
---

Sinks that allow codecs have been updated to allow analogous options to those
that were [previously added to sources][source_decoding]. This means you can
now, rather than just specifying `encoding.codec`, you can now supply custom
`framing` options. Additionally, the supported codecs (`encoding.codec`) for
each sink was expanded to be a uniform set of codecs.

For example, if you have a `socket` sink that you want to send [octet
framed][octet_framing], JSON-encoded, messages, you can now do so with
configuration like:

```toml
[sinks.socket]
type = "socket"
address = "92.12.333.224:5000"
mode = "tcp"
framing.method = "length_delimited"
encoding.codec = "json"
```

This will encode messages flowing into this sink as JSON and frame them using
[octet framing][octet_framing].

[source_decoding]: /highlights/2021-10-06-source-codecs
[octet_framing]: https://datatracker.ietf.org/doc/html/rfc6587#section-3.4.1
