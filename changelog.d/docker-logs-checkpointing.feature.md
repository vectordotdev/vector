Added checkpointing to the `docker_logs` source. Vector now resumes log collection from where it left off after a restart instead of only collecting new logs. On first start (no checkpoint), all available historical logs are collected by default. Set `since_now: true` to only capture logs produced after Vector starts. Previously, Vector was not capturing historical logs.

authors: vincentbernat
