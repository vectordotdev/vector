Replaced miniz_oxide backend for zlib compression with zlib-rs for significantly improved compression performance. zlib-rs provides 2-3x performance improvements for compression, and ~20% improvements for decompression than miniz_oxide.

authors: JakubOnderka
