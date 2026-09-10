The Windows service now reports `SERVICE_STOPPED` with a service-specific exit code to the Service Control Manager when Vector fails to start (for example, when the configuration is invalid). Previously the service was left stuck in the `START_PENDING` state indefinitely and configured service recovery actions never ran.

authors: klondikedragon
