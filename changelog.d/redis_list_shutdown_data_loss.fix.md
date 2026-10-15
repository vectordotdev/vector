The `redis` source no longer loses list entries when Vector shuts down. With `data_type: list`, shutdown could abandon a blocking pop after Redis had already removed the entry, dropping up to one entry per source on every graceful shutdown. Blocking pops now time out after one second and shutdown is checked between pops, so an idle source can take up to a second longer to stop.

authors: dsavov
