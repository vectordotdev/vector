# RFC 8447 - 2021-07-23 - Extract Helm charts out of `vectordotdev/vector` repository

Our Helm charts should be extracted from our application repository and housed in a standalone git repository to better
enables standardized tooling and processes for Helm. This extraction will resolve a number of issues with our existing
system and better align our workflows with community expectations.

## Scope

### In

- Extracting the relevant contents from `distribution/helm/` into a new git repository
- Migrating _chart_ related CI to the industry standard workflows (reference `DataDog/helm-charts`)
- Continuing to support running `test-e2e-kubernetes` suite from application repository
- Cross repository syncing (updating `appVersion` and raw resources)

### Out

- Changes to the current raw and kustomize resources
- Functional changes to existing `test-e2e-kubernetes` suite

## Pain

Introducing standard tooling like [`ct`](https://github.com/helm/chart-testing) is complicated with Vector's chart management
today. No commit in our git repository has a "functional" chart configuration, version are set to `0.0.0` (including the image
tag version), thus any usage of Helm's testing CLI `ct` would involve a custom configuration that would need to be updated for
each release (and duplicated for any test config).

Today our `test-e2e-kubernetes` suite does double duty (arguably triple) by both running smoke/integration tests and verifying
that our charts are designed and configured properly. This isn't efficient as we need to compile Vector and build an image to
test any changes to our charts which turns a process that could be a couple minutes into half an hour ([based on our CI dashboard](https://app.datadoghq.com/metric/explorer?from_ts=1627251689494&to_ts=1627445043208&live=false&tile_size=l&exp_metric=gh.actions.workflow_job.execution_secs.99percentile&exp_scope=conclusion%3Asuccess%2Cworkflow%3Ak8s_e2e_suite&exp_group=workflow&exp_agg=max&exp_row_type=metric#workflow:test_suite)).

The final issue is that today we have coupled chart releases directly to Vector releases, we publish chart versions in lockstep
which causes either delays in chart improvements (or fixes) or unnecessary Vector releases to address chart updates.

## User Experience

If we republish our stable chart releases to the new [chart-releaser](https://github.com/helm/chart-releaser) based Helm repository
we would be able to redirect https://packages.timber.io/helm/latest to the new GitHub Pages based index. This would allow for a
transition that requires no changes on the user end.

Users that have installed charts through cloning our repository and installing via their filesystem would need to clone and track
the new repository, but this method is not documented, or advised.

We can maintain backward compatibility for users referencing our raw or kustomize resources.

## Implementation

The extraction is straightforward. Migrate the contents of `distribution/helm` into a new `vectordotdev/helm-charts` git repository.

Existing CI jobs for our charts can be transferred to the new repository with minor changes, or replaced with the tooling available
with `ct`. Our existing lint job calls a make target that lints all charts in the `distribution/helm` directory and could be replaced
with the `ct lint` command, which is essentially the same but contains additional helpers.

We can leverage DataDog's [script](https://github.com/DataDog/datadog-agent/blob/main/Dockerfiles/manifests/generate.sh) or update our
existing scripts to keep the raw and kustomize resources in sync with changes to the `vectordotdev/helm-charts` repository. This could be
automated "dependabot" style by [triggering a workflow](https://docs.github.com/en/actions/reference/events-that-trigger-workflows#manual-events)
when changes are merged in the `vectordotdev/helm-charts` repository. Similarly, we can trigger workflows to update our charts' `appVersion`
whenever releases are created in the `vectordotdev/vector` repository to ensure our charts are always tracking the latest application version.
It's not a requirement to automate these steps and scripts/make targets will be used until the need to automate them arises.

The `k8s-test-framework` already contains an `external_chart` method, used for deploying DataDog's agent chart during integration tests.
This allows us to update tests that expect Vector's charts to be located in the same repository to instead reference our hosted charts
at https://packages.timber.io/helm.

Issues directly related to Vector's Helm charts (or the resources generated from them) will be maintained in the `vectordotdev/helm-charts`
repository. Existing issues will be migrated as needed.

## Rationale

This change aligns us with standard tooling and practices in the Helm community, not to mention DataDog's existing workflows. The
immediate benefit to contributors would be drastically shrinking the CI feedback loop by no longer requiring the compilation and image
building of Vector to test changes to the chart templates and configuration. This in turn allows us to start reducing the responsibilities
of the existing integration tests, which reduces the feedback time for changes to Vector that actually require integration testing on Kubernetes.

## Prior Art

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
doing nothing continues this inefficiency as we expand our testing requirements to cover a wider spread of uses.

### Version charts inside of `vectordotdev/vector`

We could create unique tags for chart versions, `chart-agent-1.0.0` for example, and keep all charts in the application
repository. This feels _hacky_ but likely would resolve the pain points with minimal adjustments to our existing workflows.

## Plan Of Attack

- [ ] Copying charts from `distribution/helm` to `vectordotdev/helm-charts` and ensure CI/CD is properly configured
- [ ] PR `vectordotdev/vector` to no longer require our charts to be local and remove `distribution/helm` directory
- [ ] Migrate any integration tests that are _only_ testing configuration into the `vectordotdev/helm-charts` repository
- [ ] Review existing issues related to the Helm charts, migrate what's still needed and close what isn't
- [ ] Migrate stable releases to new helm repository, redirect existing repo to the new repository
