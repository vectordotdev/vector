The `docker_logs` source now resumes a container's log stream from the last received log after a transient stream error. Previously it fetched again from when Vector started, which re-delivered every log of that container read since then.

authors: yussypu
