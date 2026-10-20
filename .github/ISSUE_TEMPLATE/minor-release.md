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

Keep the freeze active until **both** the release workflow has pushed its post-release housekeeping
to `master` **and** the Kubernetes manifests push below has completed. Maintainers with
bypass access must also respect this window: do not merge unrelated PRs into `master`.
[Unfreeze master](https://github.com/vectordotdev/vector/actions/workflows/release_unfreeze.yml)
closes the window automatically once both are done.

# Publish the release

- [ ] On release day, squash-merge the approved preparation PR directly into `master` using admin
      permissions (`Merge without waiting for requirements to be met (bypass rules)`). Do not use the merge queue.

The merge kicks off the [Tag approved release](https://github.com/vectordotdev/vector/actions/workflows/release_autotag.yml)
workflow, which creates the version tag and release branch at that exact commit.
The tag starts the release workflow; do not create the tag or release branch manually.

- [ ] Wait for [release workflow](https://github.com/vectordotdev/vector/actions/workflows/release.yml) to complete.
  - If it fails, use **Re-run failed jobs** to continue the release.
- [ ] Confirm that the release changelog was published to https://vector.dev/releases/
  - Refer to the internal releasing doc to monitor the deployment.
- [ ] Confirm that [Homebrew](https://github.com/vectordotdev/homebrew-brew) was released ([workflow](https://github.com/vectordotdev/homebrew-brew/actions/workflows/release.yml))
- [ ] Release Linux packages. Refer to the internal releasing doc.

- [ ] Wait for the Helm chart [Post Release](https://github.com/vectordotdev/helm-charts/actions/workflows/release-post.yml) to complete.
  - See [releasing Helm chart](https://github.com/vectordotdev/helm-charts/blob/develop/RELEASING.md).

- [ ] Wait for the Helm chart release to push the Kubernetes manifests directly to `master` ([workflow](https://github.com/vectordotdev/vector/actions/workflows/release_manifests.yml)).
- [ ] Wait for the [Unfreeze master](https://github.com/vectordotdev/vector/actions/workflows/release_unfreeze.yml) workflow to finalize the release.
