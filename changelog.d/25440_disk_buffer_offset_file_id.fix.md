Fixed a disk buffer bug that could cause Vector to read from the wrong data file after many file rotations. This occurred when a file ID calculation exceeded 65,535 and wrapped incorrectly, even if only a few buffer files existed on disk.

authors: xfocus3
