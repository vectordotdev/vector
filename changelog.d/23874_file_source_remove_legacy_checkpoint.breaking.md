Dropped support for `file` source legacy checkpoints stored in the `checkpoints` folder (Vector < 0.11) and
legacy checkpoint checksum format (Vector < 0.14). The intentionally hidden
`fingerprint.bytes` option was also removed.

### How to upgrade

Odds are the source automatically converted checkpoints to the new format and you are not affected by this at all.
To upgrade run the `file` source with any version of Vector older than `0.14`, but strictly before `0.51`.

You are only affected if you are using any of the following (and have not run the file source with
a Vector version `> 0.14` and `< 0.51`):

* You are using the legacy `checkpoints` folder (Vector < 0.11)
* Your checkpoints are using the legacy checkpoint checksum CRC format
* The `checkpoints.fingerprint[].checksum` is present in your `checkpoints.json` (instead of `first_line_checkpoint`).

authors: thomasqueirozb
