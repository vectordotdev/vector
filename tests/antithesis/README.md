# Antithesis Tests

This directory contains a sub-project to run Antithesis tests for Vector. The
`vector_disk_buffer_soundness` scenario tests the `disk_v2` contract Vector can
support today: bounded storage and accounting, integrity of delivered records,
crash-safe restart, post-fault drain, and fresh progress. It deliberately does
not treat a source response as proof that the disk buffer has fsynced the event.

The older end-to-end scenario separately probes acknowledgement conservation.
Its result must be interpreted against the acknowledgement boundary implemented
by the topology under test.

## Prerequisites

* snouty -- https://github.com/antithesishq/snouty
* antithesis-skills + claude -- https://github.com/antithesishq/antithesis-skills

## Running Scenarios

This effort is extremely early. Today we assume claude drives scenarios runs,
command it to do so with `/antithesis-launch`. In order for this to work you
must already have credentials available. Eventually we will have CI rigged up to
do nightly shots.
