# Vector Helm Chart

## Important design aspects

Our charts use Helm dependency system, however we only use local (`file://...`)
dependencies, and *no external dependencies*.

The mental model we use to manage our charts, and the automation around them
relies on this fact, so if you're introducing external dependencies know that
some of the design decisions have to be revisited.

## Development

As previously noted, our charts use Helm dependency system.

Helm vendors it's dependencies, so, when working on Helm charts, it's important
to keep the local dependencies up to date.

To aid with this task, a script `scripts/helm-dependencies-update.sh` was
created. It will update the dependencies of all our crates to each other in the
proper order, making sure the changes are propagated to all the charts.

Typical development iteration cycle looks like this:

1. Edit a file that is part of, for instance, the `vector-shared` chart, save it.
2. `scripts/helm-dependencies-update.sh`
3. `helm install vector distribution/helm/vector` (or `helm template ...`, or
   whatever you prefer to test your work).
