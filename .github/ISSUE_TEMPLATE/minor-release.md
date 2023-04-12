---
name: Vector minor release
about: Use this template for a new minor release.
title: "Vector [version] release"
labels: "domain: releasing"
---

The week before the release:

- [ ] Create a new release branch from master to freeze commits
  - `git fetch && git checkout origin/master && git checkout -b v0.<new version number> && git push -u`
- [ ] Create a new release preparation branch from `master`
  - `git checkout -b prepare-v0.<new version number> && git push -u`
- [ ] Run `cargo vdev build release-cue` to generate a new cue file for the release
- [ ] Add `changelog` key to generated cue file
  - [ ] `git log --no-merges --cherry-pick --right-only <last release tag>...`
  - [ ] Should be hand-written list of changes
        ([example](https://github.com/vectordotdev/vector/blob/9fecdc8b5c45c613de2d01d4d2aee22be3a2e570/website/cue/reference/releases/0.19.0.cue#L44))
  - [ ] Ensure any breaking changes are highlighted in the release upgrade guide
  - [ ] Ensure any deprecations are highlighted in the release upgrade guide
  - [ ] Ensure all notable features have a highlight written
  - [ ] Add description key to the generated cue file with a description of the release (see
        previous releases for examples).
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
  - [ ] `git tag v0.<minor>.0 -a -m v0.<minor>.0``
  - [ ] `git push origin v0.<minor>.0
- [ ] Wait for release workflow to complete
  - Discoverable via [https://github.com/timberio/vector/actions/workflows/release.yml](https://github.com/timberio/vector/actions/workflows/release.yml)
- [ ] Release updated Helm chart. See [releasing Helm chart](https://github.com/vectordotdev/helm-charts#releasing).
- [ ] Once Helm chart is released, updated Vector manifests
    - Run `cargo vdev build manifests` and open a PR with changes
- [ ] Add docker images to [https://github.com/DataDog/images](https://github.com/DataDog/images/tree/master/vector) to have them available internally.
- [ ] Cherry-pick any release commits from the release branch that are not on `master`, to `master`
- [ ] Bump the release number in the `Cargo.toml` on master to the next major release
- [ ] [Update Netlify deploy settings](https://app.netlify.com/sites/vector-project/settings/deploys#deploy-contexts)
  - [ ] Update production branch to the new major version
  - [ ] Update branch deploys to include the previous major version
  - [ ] Add [branch subdomain](https://app.netlify.com/sites/vector-project/settings/domain) for previous version
- [ ] Kick-off post-mortems for any regressions resolved by the release
