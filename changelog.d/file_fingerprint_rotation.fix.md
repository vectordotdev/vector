Fix a rotation race in the `file` and `kubernetes_logs` sources that could skip or duplicate logs and truncate the beginning of a record. No configuration changes are required.

authors: pznamensky
