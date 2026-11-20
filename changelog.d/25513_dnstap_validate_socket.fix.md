Fixed `vector validate` deleting the Unix socket file of a running `dnstap` source (in `mode: unix`). Socket setup for framestream-based unix sources is now performed when the source starts rather than when it is built, so validating a config no longer has destructive side effects on a running instance.

authors: xfocus3
