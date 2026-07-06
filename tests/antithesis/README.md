# Antithesis Tests

This directory contains a sub-project to run Antithesis tests for Vector. The
current focus is the `disk_v2` disk buffer: establishing that events Vector
acknowledges are conserved rather than lost across crashes, restarts, and
injected faults, and probing whether an acknowledgement's claimed
durability actually holds under those conditions.

## Prerequisites

* snouty -- https://github.com/antithesishq/snouty
* antithesis-skills + claude -- https://github.com/antithesishq/antithesis-skills

## Running Scenarios

This effort is extremely early. Today we assume claude drives scenarios runs,
command it to do so with `/antithesis-launch`. In order for this to work you
must already have credentials available. Eventually we will have CI rigged up to
do nightly shots.
