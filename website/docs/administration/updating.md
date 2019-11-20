---
title: Updating
description: Updating Vector to a later version
---

Updating Vector depends largely on your [installation][docs.installation] 
method. Each installation guide provides it's own "Updating" section:

---

**Containers**

import Jump from '@site/src/components/Jump';

<Jump to="[[[Docker][docs.containers.docker#updating]]]">Docker</Jump>

**Package managers**

<Jump to="[[[DPKG][docs.package_managers.dpkg#updating]]]">DPKG</Jump>
<Jump to="[[[Homebrew][docs.package_managers.homebrew#updating]]]">Homebrew</Jump>
<Jump to="[[[RPM][docs.package_managers.rpm#updating]]]">RPM</Jump>

**Manual**

<Jump to="/docs/setup/installation/manual/from-archives#updating">Updating from archives</Jump>
<Jump to="/docs/setup/installation/manual/from-source#updating">Updating from source</Jump>

## Working Upstream

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


[docs.containers.docker#updating]: /docs/setup/installation/containers/docker#updating
[docs.installation]: /docs/setup/installation
[docs.package_managers.dpkg#updating]: /docs/setup/installation/package-managers/dpkg#updating
[docs.package_managers.homebrew#updating]: /docs/setup/installation/package-managers/homebrew#updating
[docs.package_managers.rpm#updating]: /docs/setup/installation/package-managers/rpm#updating
[docs.topologies]: /docs/setup/deployment/topologies
