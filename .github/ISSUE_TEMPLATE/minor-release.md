---
name: Vector minor release
about: Use this template for a new minor release.
title: "Vector [version] release"
labels: "domain: releasing"
---

The week before the release:

- [ ] Check for any outstanding deprecation actions in [DEPRECATIONS.md](docs/DEPRECATIONS.md) and
      take them (or have someone help you take them)
- [ ] Create a new release branch from master to freeze commits
  - `git fetch && git checkout origin/master && git checkout -b v0.<new version number> && git push -u`
- [ ] Create a new release preparation branch from `master`
  - `git checkout -b prepare-v0.<new version number> && git push -u`
- [ ] Check if there is a newer version of Alpine or Debian available to update the release images
      in `distribution/docker/`. Update if so.
- [ ] Run `cargo vdev build release-cue` to generate a new cue file for the release
  - [ ] Add description key to the generated cue file with a description of the release (see
        previous releases for examples).
  - [ ] Ensure any breaking changes are highlighted in the release upgrade guide
  - [ ] Ensure any deprecations are highlighted in the release upgrade guide
- [ ] Update version number in `website/cue/reference/administration/interfaces/kubectl.cue`
- [ ] Update version number in `distribution/install.sh`
- [ ] Add new version to `website/cue/reference/versions.cue`
- [ ] Create new release md file by copying an existing one in `./website/content/en/releases/` and
      updating version number
- [ ] Commit these changes
- [ ] Open PR against the release branch (`v0.<new version number>`) for review
- [ ] PR approval

On the day of release:

- [ ] Rebase the release preparation branch on the release branch
    - [ ] Squash the release preparation commits (but not the cherry-picked commits!) to a single
        commit. This makes it easier to cherry-pick to master after the release. 
    - [ ] Ensure release date in cue matches current date.
- [ ] Merge release preparation branch into the release branch
    - `git co v0.<new version number> && git merge --ff-only prepare-v0.<new version number>`
- [ ] Tag new release
  - [ ] `git tag v0.<minor>.0 -a -m v0.<minor>.0`
  - [ ] `git push origin v0.<minor>.0`
- [ ] Wait for release workflow to complete
  - Discoverable via [https://github.com/timberio/vector/actions/workflows/release.yml](https://github.com/timberio/vector/actions/workflows/release.yml)
- [ ] Release Linux packages. See [`vector-release` usage](https://github.com/DataDog/vector-release#usage).
- [ ] Release updated Helm chart. See [releasing Helm chart](https://github.com/vectordotdev/helm-charts#releasing).
- [ ] Once Helm chart is released, updated Vector manifests
    - Run `cargo vdev build manifests` and open a PR with changes
- [ ] Add docker images to [https://github.com/DataDog/images](https://github.com/DataDog/images/tree/master/vector) to have them available internally.
- [ ] Cherry-pick any release commits from the release branch that are not on `master`, to `master`
- [ ] Bump the release number in the `Cargo.toml` on master to the next major release
- [ ] Drop a note in the #websites Slack channel to request an update of the branch deployed
      at https://vector.dev to the new release branch.
- [ ] Kick-off post-mortems for any regressions resolved by the release
