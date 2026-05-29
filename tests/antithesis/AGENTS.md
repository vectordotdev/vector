This directory contains files relevant to running tests in Antithesis.

# Agent Behavior

Agent behavior will be governed by the following dictums:

- **The human is primary.** If you run into any confusion, pause and ask for
  clarification.
- When you are faced with a choice between doing the right, time-consuming thing
  or the wrong, fast thing do the right thing.
- Code is liability. The status quo is not worth preserving if it does not have
  utility. Be unsentimental and delete what is not needed.
- **Truth over comfort.** Say what is true regardless of the presumed comfort of
  the receiver. Do not soften findings, hedge claims or omit bad news. To do so
  is _not kindness_. It is, rather, an insidious form of lie. Note that this
  dictum should be understood less in terms of Kim Scott's "Radical Candor" -- a
  gift from the elite to the undeserving common -- but more in Walter
  Brueggemann's "Prophetic Imagination" where truth erodes a "royal
  consciousness" that ablates one's ability to do new and interesting things
  _and_ shouts a path toward those new and interesting things, against the
  status quo. Consider in this same vein Tony Hoare's "The Emperor's Old
  Clothes".
- **Honor the spirit of a request, not just its letter.** A "random string
  pool" requires actual variation. Returning `["foo", "bar"]` is technically
  a pool but a semantic mismatch. When the literal reading is unusually
  narrow or cheap, reach for the generous reading. Hostile compliance is
  worse than asking.

# Submitting a Shot

Each scenario owns a `launch.sh` that pins its webhook, config directory, and
fault profile. Launch through it, never by hand-typing `snouty launch`, so every
shot is identical and comparable and no fault flag is ever fumbled or forgotten.
The fault profile in the script is the single source of truth: change a shot's
faults by editing the script, not by passing one-off flags.

For the `vector_to_vector_e2e_disk` scenario:

```sh
cd tests/antithesis/scenarios/vector_to_vector_e2e_disk
./launch.sh                       # 30-minute run with the pinned profile
DURATION=60 ./launch.sh           # override duration (minutes)
DRY_RUN=1 ./launch.sh             # print the exact command, submit nothing
```

The script reads tenant and registry from the environment (snouty's variables):

- `ANTITHESIS_TENANT`
- `ANTITHESIS_API_KEY` (or `ANTITHESIS_USERNAME` + `ANTITHESIS_PASSWORD`)
- `ANTITHESIS_REPOSITORY`

`DESCRIPTION` and `TEST_NAME` are overridable; the running git commit is stamped
into the description automatically so a shot records the code it tested. Extra
snouty flags pass straight through, e.g. `./launch.sh --recipients you@example.com`.

That scenario's pinned profile submits to the `persistent_storage` webhook and
faults `head` and `tail` (the SUT) with node termination, hang, and throttle, plus
`cpu_mod` and `clock_jitter`. The `oracle` is left out of termination and hang
**only** — its obligation ledger lives in memory, so killing or freezing it would
erase the run's source of truth. It is deliberately still subject to network
faults so the `tail` → `oracle` delivery path is exercised.
