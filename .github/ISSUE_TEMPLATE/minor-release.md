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
- [ ] Set the `Release freeze` ruleset to **Active** in [repository rulesets](https://github.com/vectordotdev/vector/settings/rules).
- [ ] Run the [Prepare release](https://github.com/vectordotdev/vector/actions/workflows/release_prepare.yml)
      workflow from `master` with `version` set to the stable Vector version and `vrl_version` to the exact released VRL version.
- [ ] Review the bot-authored `prepare-v-<major>-<minor>-<patch>-website` PR: edit the release description,
      changelog, upgrade guidance, and release date as needed. Review deprecations with
      `cargo vdev deprecation show --version "${NEW_VECTOR_VERSION}"`.

Keep the freeze active until housekeeping has merged. Maintainers with bypass access must also
respect this window: do not merge unrelated PRs into `master`.

# Publish the release

- [ ] On release day, squash-merge the approved preparation PR directly into `master` using the
      release-freeze bypass. Do not use the merge queue.

The merge approves the release. [Tag approved release](https://github.com/vectordotdev/vector/actions/workflows/release_autotag.yml)
validates the squash-merge commit and creates the version tag and release branch at that exact commit.
The tag starts the release workflow; do not create the tag or release branch manually.

- [ ] Wait for release workflow to complete.
  - Discoverable via [release.yml](https://github.com/vectordotdev/vector/actions/workflows/release.yml)
- [ ] Reset the `website` branch to the `HEAD` of the release branch to update https://vector.dev
  - [ ] `git fetch origin && git switch website && git reset --hard origin/"${RELEASE_BRANCH}" && git push --force-with-lease`
  - [ ] Confirm that the release changelog was published to https://vector.dev/releases/
    - Refer to the internal releasing doc to monitor the deployment.
- [ ] Release Linux packages. Refer to the internal releasing doc.
- [ ] Release updated Helm chart. See [releasing Helm chart](https://github.com/vectordotdev/helm-charts/blob/develop/RELEASING.md).
- [ ] Release Homebrew. Refer to the internal releasing doc.
- [ ] Update the latest [release tag](https://github.com/vectordotdev/vector/releases) description with the release announcement.

# Post-release housekeeping

- [ ] Wait for the release workflow to merge its housekeeping PR after checks pass.
      It begins the next minor `-dev` version, restores VRL `main`, and refreshes licenses and documentation.
- [ ] Set the `Release freeze` ruleset back to **Disabled**.
- [ ] Run `cargo vdev build manifests` after the Helm chart release and open a separate PR with the changes.
