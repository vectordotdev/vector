Fixed `vdev check docs` reporting unrelated dirty working tree files after running `cue fmt`. The check now snapshots CUE files before and after formatting so only files actually modified by the formatter are reported.

authors: patrickswedish
