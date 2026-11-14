`vector tap` now exits cleanly when its output pipe is closed, for example `vector tap | head -1`, instead of panicking with "failed printing to stdout: Broken pipe".

authors: anpawo
