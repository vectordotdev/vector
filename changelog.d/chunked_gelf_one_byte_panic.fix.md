Fixed a panic in the `chunked_gelf` framing decoder when a one-byte message arrived and trace-level logging was enabled for it, which took down the source. Such a message is now passed on for the decoder to reject, as any other malformed payload would be.

authors: pront
