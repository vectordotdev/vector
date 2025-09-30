Support for `file` source legacy checkpoints stored in the `checkpoints` folder (Vector < 0.11) and
legacy checkpoint checksum format (Vector < 0.14) was dropped.

### How to upgrade:

If you are using the legacy `checkpoints` folder or legacy checkpoint checksum format, run the `file`
source with any version of Vector older than `0.14`, but strictly before `0.51`. Odds are the source
automatically converted checkpoints to the new format and you are not affected by this at all.
