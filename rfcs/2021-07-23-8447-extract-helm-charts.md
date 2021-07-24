# RFC 8447 - 2021-07-23 - Extract Helm charts out of `timberio/vector` repository

Our Helm charts should be extracted from our application repository and housed in a standalone git repository to better
enabled standardized tooling and processes for Helm. This extraction will resolve a number of issues with out existing
system and better align our workflows with community expectations.

## Scope

### In

- Extracting the relevant contents from `distribution/helm/` into a standalone git repository
- Migrating _chart_ related CI to new git repository (lint, kubeval, _config_ test, etc)
- Continuing to support running `test-e2e-kubernetes` suite from application repository
- Cross repository syncing (updating `appVersion` and raw resources)

### Out

- Incompatible changes to the raw and kustomize resources
- Functional changes to existing `test-e2e-kubernetes` suite
- Migration to existing `DataDog/helm-charts` git repository
- Changes to existing _chart_ repository hosted in S3

## Pain

Introducing standard tooling like [`ct`](https://github.com/helm/chart-testing) is troublesome at best, and unfeasible
at worst with Vector's chart management today. No commit in our git repository has a "functional" chart configuration, version
are set to `0.0.0` (including the image tag version), thus any usage of `ct` would involve a custom configuration that would
need to be updated for each release (and duplicated for any test config).

Today our `test-e2e-kubernetes` suite does double duty (possibly triple) by both running smoke/integration tests and verifying
that our charts are designed and configured properly. This isn't efficient as we need to compile Vector and build an image to
test any changes to our charts which turns a process that could be a couple minutes into half an hour (based on our CI dashboard).

The final issue is that today we have coupled chart releases directly to Vector releases, we publish chart versions in lockstep
which causes either delays in chart improvements (or fixes) or unnecessary Vector releases to address chart updates.

## User Experience

Assuming our charts are being consumed as _documented_ there should be no end user impact. The `helm repo` URL will remain the same
and already published chart versions will be unchanged.

This does not change the contents of the charts so any published charts will not be functionally different from releases prior to
extracting the charts to their own git repository.

Users that have installed charts through cloning our repository and installing via their filesystem would need to clone and track
the new repository, but this method is not advised, documented, or advised.

We can maintain backward compatibility for users referencing our raw or kustomize resources.

## Implementation

The extraction is simple. Create a new `timberio/helm-charts` repository and migrate the contents of `distribution/helm` into the
new repository.

Existing CI jobs for our charts can be transfered to the new repository with minor changes, or replaced with the tooling available
with `ct`. Our existing lint job calls a make target that lints all charts in the `distribution/helm` directory and could be replaced
with the `ct lint` command, which is essentially the same but contains additional helpers.

We can leverage DataDog's [script](https://github.com/DataDog/datadog-agent/blob/main/Dockerfiles/manifests/generate.sh) or update our
existing scripts to keep the raw and kustomize resources in sync with changes to the `timberio/helm-charts` repository. This could be
automated "dependabot" style by [triggering a workflow](https://docs.github.com/en/actions/reference/events-that-trigger-workflows#manual-events)
when changes are merged in the `timberio/helm-charts` repository. Similarly, we can trigger workflows to update our charts' `appVersion`
whenever releases are created in the `timberio/vector` repository to ensure our charts are always tracking the latest application version.
It's not a requirement to automate these steps and scripts/make targets can be used instead if we decide to reduce scope and automate
these tasks at a later time.

The `k8s-test-framework` already contains an `external_chart` method, used for deploying DataDog's agent chart during integration tests.
This allows us to update tests that expect Vector's charts to be located in the same repository to instead reference our hosted charts
at https://packages.timber.io/helm.

## Rationale

This change aligns us with standard tooling and practices in the Helm community, not to mention DataDog's existing workflows. The
immediate benefit to contributors would be drastically shrinking the CI feedback loop by no longer requiring the compilation and image
building of Vector to test changes to the chart templates and configuration. This in turn allows us to start reducing the responsibilities
of the existing integration tests, which reduces the feedback time for changes to Vector that actually require integration testing on Kubernetes.

## Prior Art

There's nothing about our situation that is unique and we should follow established patterns for Helm charts.

### DataDog Agent

Helm charts are stored separately in [DataDog/helm-charts](https://github.com/DataDog/helm-charts), with raw manifests
stored in the application repository generated with a [script](https://github.com/DataDog/datadog-agent/blob/main/Dockerfiles/manifests/generate.sh).

## Drawbacks

Extracting our Helm charts into their own repository adds additional management overhead, as well as possible context
switching for contributors. Specifically it creates bidirectional dependencies between the application and chart repositories
as we need to keep the `appVersion` in sync with Vector releases, and the Kubernetes manifests in sync with the chart releases.

Triggering cross-repository workflows in GitHub Actions isn't well supported but appears possible. This could impact our
ability to automate updates between the application and chart repositories.

## Alternatives

### Do Nothing

Our existing setup forces our testing strategy to rely on costly compilation and building steps for even minor changes,
doing nothing continues this inefficiency as we expand our testing requirements to cover a wider spread of environments
(validating usage across major cloud providers).

### Version charts inside of `timberio/vector`

We could create unique tags for chart versions, `chart-agent-1.0.0` for example, and keep all charts in the application
repository. This feels _hacky_ but likely would resolve the pain points with minimal adjustments to our existing workflows.
I anticipate in the long term Vector charts will be migrated to the `DataDog/helm-charts` repository, while this solution
shouldn't block any such migration it also doesn't inherently support it.

## Outstanding Questions

- Do we need to automate the cross-repository management or just document the steps with the initial work?

## Plan Of Attack

- [ ] Create `timberio/helm-charts` repository, copying charts from `distribution/helm` and configuring CI for testing and release
- [ ] PR `timberio/vector` to no longer require our charts to be local and remove `distribution/helm` directory
- [ ] Migrate any integration tests that are _only_ testing configuration into the new `timberio/helm-charts` repository
