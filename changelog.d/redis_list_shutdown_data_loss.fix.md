The `redis` source no longer loses list entries when Vector shuts down. With `data_type: list`, shutdown could abandon a blocking pop after Redis had already removed the entry, dropping up to one entry per source on every graceful shutdown. Shutdown now lets an in-flight pop finish (waiting at most five seconds if Redis stops responding), and blocking pops time out after one second so they never hold shutdown up on a healthy connection.

authors: dsavov
