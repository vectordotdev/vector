---
name: Vector patch release
about: Use this template for a new patch release.
title: "Vector [version] release"
labels: "domain: releasing"
---

# Setup environment

```shell
export CURRENT_MINOR_VERSION = <current minor version> # e.g. 47
export CURRENT_PATCH_VERSION = <current patch version> # e.g. 0
export CURRENT_VERSION="${RELEASE_BRANCH}"."${CURRENT_PATCH_VERSION}"
export NEW_PATCH_VERSION = <new patch version> # e.g. 1
export NEW_VERSION="${RELEASE_BRANCH}"."${NEW_PATCH_VERSION}"
export RELEASE_BRANCH=v0."${CURRENT_MINOR_VERSION}"
export PREP_BRANCH=prepare-v-0-"${CURRENT_MINOR_VERSION}"-"${NEW_PATCH_VERSION}"-website
```

# Before the release

- [ ] Create a new release preparation branch from the current release branch
  - `git fetch --all && git checkout "${RELEASE_BRANCH}" && git checkout -b "${PREP_BRANCH}""`
- [ ] Cherry-pick in all commits to be released from the associated release milestone
  - If any merge conflicts occur, attempt to solve them and if needed enlist the aid of those familiar with the conflicting commits.
- [ ] Bump the release number in the `Cargo.toml` to the current version number
- [ ] Add a new cue file for the release at `website/cue/reference/releases/${NEW_VERSION}.cue`
      by copying the previous patch release file and editing the version, date, commits, and
      changelog entries to match this release.
  - [ ] Add a description key to the cue file with a description of the release (see
        previous releases for examples).
- [ ] Update version number in `distribution/install.sh`
- [ ] Add new version to `website/cue/reference/versions.cue`
- [ ] Create new release md file by copying an existing one in `./website/content/en/releases/`.
  - Update the version number to `"${NEW_VERSION}"` and increase the `weight` by 1.
- [ ] Run `cargo check` to regenerate `Cargo.lock` file
- [ ] Commit these changes
- [ ] Open PR against the release branch (`"${RELEASE_BRANCH}"`) for review
- [ ] PR approval

# On the day of release

- [ ] Ensure release date in cue matches current date.
- [ ] Rebase the release preparation branch on the release branch
  - Squash the release preparation commits (but not the cherry-picked commits!) to a single
    commit. This makes it easier to cherry-pick to master after the release.
  - `git fetch --all && git checkout website-prepare-v0-"${CURRENT_MINOR_VERSION}"-"${NEW_PATCH_VERSION}" && git rebase -i "${RELEASE_BRANCH}"`
- [ ] Merge release preparation branch into the release branch
  - `git checkout "${RELEASE_BRANCH}" && git merge --ff-only website-prepare-v0-"${CURRENT_MINOR_VERSION}"-"${NEW_PATCH_VERSION}"`
- [ ] Tag new release
  - [ ] `git tag "${NEW_VERSION}" -a -m "${NEW_VERSION}"`
  - [ ] `git push origin "${NEW_VERSION}"`
- [ ] Wait for release workflow to complete
  - Discoverable via [https://github.com/timberio/vector/actions/workflows/release.yml](https://github.com/timberio/vector/actions/workflows/release.yml)
- [ ] Release Linux packages. See [`vector-release` usage](https://github.com/DataDog/vector-release#usage).
  - Note: the pipeline inputs are the version number `"${CURRENT_VERSION}"` and a personal GitHub token.
  - [ ] Manually trigger the `trigger-package-release-pipeline-prod-stable` job.
- [ ] Push the release branch to update the remote (This should close the preparation branch PR).
  - `git checkout "${RELEASE_BRANCH}" && git push`
- [ ] Review and squash-merge the Helm release PR, then wait for the chart release.
  - The Vector release workflow starts [Helm release preparation](https://github.com/vectordotdev/helm-charts/actions/workflows/release-prepare.yml)
    automatically for the latest stable Vector release.
  - See [releasing Helm chart](https://github.com/vectordotdev/helm-charts/blob/develop/RELEASING.md) for the review steps.
- [ ] Once the Helm chart is released, wait for the Kubernetes manifests push to `master`.
  - The chart release triggers [Refresh Kubernetes manifests](https://github.com/vectordotdev/vector/actions/workflows/release_manifests.yml),
    which runs `cargo vdev build manifests` and, when the generated manifests differ,
    commits and pushes them to `master` itself as the `vectordotdev-bot` — no PR, no review,
    no merge queue. If the run reports no changes, the manifests already match the chart.
- [ ] Wait for the [Unfreeze master](https://github.com/vectordotdev/vector/actions/workflows/release_unfreeze.yml)
      workflow to close the direct-push window after the manifests run succeeds.
  - It removes the temporary `vectordotdev-bot` **Always** bypass from every ruleset in
    `RELEASE_FREEZE_BOT_BYPASS`, then sets the `RELEASE_FREEZE_RULESET_ID` ruleset back to **Disabled**.
    It waits for any pending release or manifests run and for open `vectordotdev-bot` PRs first,
    and gives up after ten minutes, leaving the freeze active.
  - Run it manually with `workflow_dispatch` if the release never starts the manifests workflow, or to retry a failed run.

- [ ] Cherry-pick any release commits from the release branch that are not on `master`, to `master`
- [ ] Wait for the release workflow to reset the `website` branch to the release commit
      (`refs/heads/website` is force-pushed to the release branch HEAD) to update
      https://vector.dev with the patch release notes
- [ ] Kick-off post-mortems for any regressions resolved by the release
