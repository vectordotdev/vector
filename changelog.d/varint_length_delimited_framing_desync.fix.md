Fixed a framing bug in the `varint_length_delimited` decoder that corrupted streams whenever a
frame was split across two reads from the underlying source. The decoder consumed the varint length
prefix before confirming that the whole frame had been buffered, so a partial frame permanently
desynchronized the stream: the first byte of the payload was then misread as the next length
prefix. Any source using `framing.method = "varint_length_delimited"` was affected once the stream
exceeded the 8 KiB read buffer, unless the frame size happened to tile that buffer exactly.

authors: meirdev
