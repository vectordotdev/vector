All TCP based socket sinks now gracefully handle config reloads under load. Previously, when a configuration reload occurred and data was flowing through the topology, the vector process crashed due to the TCP sink attempting to access the stream when it had been terminated.

authors: neuronull
