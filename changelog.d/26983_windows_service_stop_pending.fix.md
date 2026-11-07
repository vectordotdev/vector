Vector running as a Windows service now reports `SERVICE_STOP_PENDING` while it shuts down, with the graceful shutdown limit as the wait hint, or the longest wait hint when shutdown has no limit. Previously the service stayed `RUNNING` until it stopped, so `Stop-Service` and `Restart-Service` failed after two seconds while Vector was still shutting down.

authors: AdzerKI
