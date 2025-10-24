The `docker_logs` source now includes exponential backoff retry logic for Docker daemon communication failures. This improves reliability when working with slow or temporarily unresponsive Docker daemons by retrying with increasing delays instead of immediately stopping.

authors: titaneric
