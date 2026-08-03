The `file` source no longer logs a "Currently ignoring file too small to fingerprint" warning for empty files when using the `checksum` fingerprinting strategy. Empty files are common and transient (for example freshly-created log files or short-lived Kubernetes pods), so the warning created noise without being actionable. Non-empty files that are still too small to fingerprint continue to warn as before.

authors: gaurav0107
