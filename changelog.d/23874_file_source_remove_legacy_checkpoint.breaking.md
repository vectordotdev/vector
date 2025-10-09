Dropped support for `file` source legacy checkpoints stored in the `checkpoints` folder (Vector < `0.11`) and
legacy checkpoint checksum format (Vector < `0.15`). The intentionally hidden
`fingerprint.bytes` option was also removed.

### How to upgrade

You can stop reading if you
* have reset your file checkpoints on or after version `0.15`, or
* don't care about the current state of your checkpoints.
  * Vector will re-read all files from the beginning if/when the `checkpoints.json` file fails to load.

You are only affected if you are using any of the following:

1. (Vector < `0.15`) Your checkpoints are using the legacy checkpoint checksum CRC format.
2. (Vector < `0.15`) The `checksum` key is present under `checkpoints.fingerprint` in your `checkpoints.json` (instead of `first_lines_checksum`).
3. (Vector < `0.11`) You are using the legacy `checkpoints` folder and/or the `unknown` key is present under `checkpoints.fingerprint` in your `checkpoints.json`.

If you are affected by `#1` or `#2`, odds are the source automatically converted checkpoints to the new format and you are not affected by this at all. To upgrade run the `file` source with any version of Vector >= `0.15`, but strictly before `0.51` and the `checkpoints.json` file should be automatically updated.

When upgrading, we recommend stepping through minor versions as these can each contain breaking changes while Vector is pre-1.0. These breaking changes are noted in their respective upgrade guides.

For example, if you’re on Vector `0.10` and want to upgrade, keep upgrading Vector until `0.14` and Vector will automatically convert your checkpoints.

If you are affected by `#3` you should manually delete the `unknown` checkpoint records from
`checkpoints.json` (if you were using a recent version of Vector and the key was still there it
wasn't being used) and then follow the upgrade guide for `#1` and `#2`.

authors: thomasqueirozb
