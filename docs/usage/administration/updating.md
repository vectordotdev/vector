---
description: Updating Vector to a later version
---

# Updating

Updating Vector depends largely on your [installation][docs.installation] 
method. Each installation guide provides it's own "Updating" section:

1. Platforms
   1. [Updating Docker][docs.platforms.docker#updating]
2. Package Managers
   1. [Updating with DPKG][docs.dpkg#updating]
   2. [Updating with Homebrew][docs.homebrew#updating]
   3. [Updating with RPM][docs.rpm#updating]
3. Manual
   1. [Updating from archives][docs.from_archives#updating]
   1. [Updating from source][docs.from_archives#updating]

## Working Upstream

![Where To Start Example][assets.updating-upstream]

Depending on your [topology][docs.topologies], you'll want update your Vector
instances in a specific order. You should _always_ start downstream and work
your way upstream. This allows for incremental updating across your topology,
ensuring downstream Vector instances do not receive data in formats that are
unrecognized. Vector always makes a best effort to successfully process data,
but there is no guarantee of this if a Vector instance is handling a data
format defined by a future unknown Vector version.

## Capacity Planning

Because you'll be taking Vector instances offline for a short period of time,
upstream data will accumulate and buffer. To avoid overloading your instances,
you'll want to make sure you have enough capacity to handle the surplus of
data. We recommend provisioning at least 20% of head room, on all resources,
to account for spikes and updating.


[assets.updating-upstream]: ../../assets/updating-upstream.svg
[docs.dpkg#updating]: ../../setup/installation/package-managers/dpkg.md#updating
[docs.from_archives#updating]: ../../setup/installation/manual/from-archives.md#updating
[docs.homebrew#updating]: ../../setup/installation/package-managers/homebrew.md#updating
[docs.installation]: ../../setup/installation
[docs.platforms.docker#updating]: ../../setup/installation/platforms/docker.md#updating
[docs.rpm#updating]: ../../setup/installation/package-managers/rpm.md#updating
[docs.topologies]: ../../setup/deployment/topologies.md
