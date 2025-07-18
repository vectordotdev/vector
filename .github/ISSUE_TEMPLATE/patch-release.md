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
- [ ] Run `cargo vdev build release-cue` to generate a new cue file for the release
  - [ ] Add description key to the generated cue file with a description of the release (see
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
- [ ] Release updated Helm chart. See [releasing Helm chart](https://github.com/vectordotdev/helm-charts#releasing).
- [ ] Once Helm chart is released, updated Vector manifests
  - Run `cargo vdev build manifests` and open a PR with changes
- [ ] Add docker images to [https://github.com/DataDog/images](https://github.com/DataDog/images/tree/master/vector) to have them available internally.
  - Follow the [instructions at the top of the mirror.yaml file](https://github.com/DataDog/images/blob/fbf12868e90d52e513ebca0389610dea8a3c7e1a/mirror.yaml#L33-L49).
- [ ] Cherry-pick any release commits from the release branch that are not on `master`, to `master`
- [ ] Reset the `website` branch to the `HEAD` of the release branch to update https://vector.dev
  - [ ] `git checkout website && git reset --hard origin/"${RELEASE_BRANCH}" && git push`
- [ ] Kick-off post-mortems for any regressions resolved by the release
