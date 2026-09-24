---
name: Vector minor release
about: Use this template for a new minor release.
title: "Vector [version] release"
labels: "domain: releasing"
---

# Prepare the release

This checklist is for stable minor releases. Set the version for the commands below:

```shell
export NEW_VECTOR_VERSION=0.59.0 # Replace with the version being released.
export RELEASE_BRANCH="v${NEW_VECTOR_VERSION%.*}"
```

- [ ] Cut a new release of [VRL](https://github.com/vectordotdev/vrl) if needed.
  - VRL release steps: https://github.com/vectordotdev/vrl/blob/main/release/README.md
- [ ] Run the [Prepare release](https://github.com/vectordotdev/vector/actions/workflows/release_prepare.yml)
      workflow from `master` with `version` set to the stable Vector version and `vrl_version` to the exact released VRL version.
  - The workflow activates the `RELEASE_FREEZE_RULESET_ID` ruleset and grants `vectordotdev-bot` an **Always** bypass
    to some of `master`'s rulesets (`RELEASE_FREEZE_BOT_BYPASS`).
  - If preparation fails after activation, the freeze remains active. Retry, or run
    [Unfreeze master](https://github.com/vectordotdev/vector/actions/workflows/release_unfreeze.yml)
    with `workflow_dispatch` to unfreeze the repository.
- [ ] Review the bot-authored `prepare-v-<major>-<minor>-<patch>-website` PR: edit the release description,
      changelog, upgrade guidance, and release date as needed. Review deprecations with
      `cargo vdev deprecation show --version "${NEW_VECTOR_VERSION}"`.

Keep the freeze active until **both** the release workflow's housekeeping PR has
merged **and** the Kubernetes manifests push below has completed. Maintainers with
bypass access must also respect this window: do not merge unrelated PRs into `master`.
[Unfreeze master](https://github.com/vectordotdev/vector/actions/workflows/release_unfreeze.yml)
closes the window automatically once both are done.

# Publish the release

- [ ] On release day, squash-merge the approved preparation PR directly into `master` using the
      release-freeze bypass. Do not use the merge queue.

The merge approves the release. [Tag approved release](https://github.com/vectordotdev/vector/actions/workflows/release_autotag.yml)
validates the squash-merge commit and creates the version tag and release branch at that exact commit.
The tag starts the release workflow; do not create the tag or release branch manually.

- [ ] Wait for release workflow to complete.
  - Discoverable via [release.yml](https://github.com/vectordotdev/vector/actions/workflows/release.yml)
- [ ] Wait for the release workflow to reset the `website` branch to the release commit,
      publishing the release notes to https://vector.dev
  - [ ] Confirm that the release changelog was published to https://vector.dev/releases/
    - Refer to the internal releasing doc to monitor the deployment.
- [ ] Release Linux packages. Refer to the internal releasing doc.
- [ ] Review and squash-merge the Helm release PR, then wait for the chart release.
  - The Vector release workflow starts [Helm release preparation](https://github.com/vectordotdev/helm-charts/actions/workflows/release-prepare.yml)
    automatically for the latest stable Vector release.
  - See [releasing Helm chart](https://github.com/vectordotdev/helm-charts/blob/develop/RELEASING.md) for the review steps.
- [ ] Release Homebrew. Refer to the internal releasing doc.
- [ ] Update the latest [release tag](https://github.com/vectordotdev/vector/releases) description with the release announcement.

# Post-release housekeeping

- [ ] Wait for the release workflow to merge its housekeeping PR after checks pass.
      It begins the next minor `-dev` version, restores VRL `main`, and refreshes licenses and documentation.
- [ ] Wait for the Helm chart release to push the Kubernetes manifests directly to `master`.
  - The chart release triggers [Refresh Kubernetes manifests](https://github.com/vectordotdev/vector/actions/workflows/release_manifests.yml),
    which runs `cargo vdev build manifests` and, when the generated manifests differ,
    commits and pushes them to `master` itself as the `vectordotdev-bot` — no PR, no review,
    no merge queue. If the run reports no changes, the manifests already match the chart.
- [ ] Wait for the [Unfreeze master](https://github.com/vectordotdev/vector/actions/workflows/release_unfreeze.yml)
      workflow to close the direct-push window after the manifests run succeeds.
  - It removes the temporary `vectordotdev-bot` **Always** bypass from every ruleset in
    `RELEASE_FREEZE_BOT_BYPASS`, then sets the `RELEASE_FREEZE_RULESET_ID` ruleset back to **Disabled**.
    It waits for the housekeeping PR to merge and for any pending release or manifests run first,
    and gives up after ten minutes, leaving the freeze active.
  - Run it manually with `workflow_dispatch` if the release never starts the manifests workflow, if that
    run fails, or to retry a failed closeout.
    A manual run makes the same checks unless you set `force`, which closes the window anyway.
