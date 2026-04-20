The release changelog generator (`scripts/generate-release-cue.rb`) now emits
fragment descriptions using CUE raw multi-line strings (`#"""..."""#`) so that
backslashes (e.g. shell line continuations) in a fragment are not interpreted
as escape sequences by `cue export`.
