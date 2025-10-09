* Dropped support for `file` source legacy checkpoints stored in the `checkpoints` folder (Vector `< 0.11`) which is located inside the `data_dir`.
* Removed the legacy checkpoint checksum format (Vector `< 0.15`).
* The intentionally hidden `fingerprint.bytes` option was also removed.

### How to upgrade

You can stop reading if you

* have started using the `file` source on or after version `0.15`, or
* have cleared your `data_dir` on or after version `0.15`, or
* don't care about the file positions and don't care about current state of your checkpoints, meaning you accept that files could be read from the beginning again after the upgrade.
  * Vector will re-read all files from the beginning if/when any `checkpoints.json` files nested inside `data_dir` fail to load due to legacy/corrupted data.

You are only affected if your Vector version is:

1. `>= 0.11` and `< 0.15`, then your checkpoints are using the legacy checkpoint checksum CRC format.
2. `>= 0.11` and `< 0.15`, then the `checksum` key is present under `checkpoints.fingerprint` in your `checkpoints.json` (instead of `first_lines_checksum`).
3. **or ever was** `< 0.11` and you are using the legacy `checkpoints` folder and/or the `unknown` key is present under `checkpoints.fingerprint` in any `checkpoints.json` files nested inside `data_dir`.

#### If you are affected by `#1` or `#2`

Run the `file` source with any version of Vector `>= 0.15`, but strictly before `0.51` and the checkpoints should be automatically updated.
For example, if you’re on Vector `0.10` and want to upgrade, keep upgrading Vector until `0.14` and Vector will automatically convert your checkpoints.
When upgrading, we recommend stepping through minor versions as these can each contain breaking changes while Vector is pre-1.0. These breaking changes are noted in their respective upgrade guides.

Odds are the `file` source automatically converted checkpoints to the new format if you are using a recent version and you are not affected by this at all.
#### If you are affected by `#3`

You should manually delete the `unknown` checkpoint records from all `checkpoints.json` files nested inside `data_dir`
and then follow the upgrade guide for `#1` and `#2`. If you were using a recent version of Vector and `unknown`
was present it wasn't being used anyways.

authors: thomasqueirozb
