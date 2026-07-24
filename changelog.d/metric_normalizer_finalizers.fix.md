Fix metric normalization retaining delivery finalizers in cached state, which could prevent sink buffers from being acknowledged and reclaimed for metric sinks such as `prometheus_remote_write`.

authors: fpytloun
