This release fixes a panic in the disk buffer that would crash Vector when encountering I/O errors such as missing buffer files. I/O errors are now treated as recoverable - Vector logs an error, emits metrics, and continues processing from the next available file.

authors: huseynsnmz
