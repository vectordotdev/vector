The OpenTelemetry source HTTP listener no longer terminates when `accept` fails with a transient error (e.g., too many open file descriptors). It now retries after one second and logs the error, preventing silent listener death under resource pressure.

authors: fbs
