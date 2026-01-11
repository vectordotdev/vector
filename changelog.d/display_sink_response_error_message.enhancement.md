Added HTTP response body previews to error logs. When an HTTP sink request fails, Vector will now attempt to decompress (gzip, zstd, br, deflate) and log the first 1024 characters of the response body to help troubleshooting.

authors: Keuin
