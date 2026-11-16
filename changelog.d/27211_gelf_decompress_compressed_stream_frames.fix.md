The GELF decoder now accepts gzip/zlib-compressed messages on stream transports (e.g. GELF over TCP from the `socket` source) instead of failing JSON parsing and dropping them, matching how the same messages are already handled on datagram transports. A new `gelf.decompression` decoding option controls this (`Auto` by default, `None` to reject compressed messages).

authors: yackushevas
