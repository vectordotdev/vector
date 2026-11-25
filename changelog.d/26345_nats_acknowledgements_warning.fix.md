The `nats` source no longer reports that it supports end-to-end acknowledgements, which it does not
implement. A sink with `acknowledgements.enabled: true` downstream of it now logs the "Silent data loss
could occur" warning at startup, as for other sources without acknowledgement support.

authors: anpawo
