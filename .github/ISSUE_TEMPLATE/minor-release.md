---
name: Vector minor release
about: Use this template for a new minor release.
title: "Vector [version] release"
labels: "domain: releasing"
---

The week before the release:

- [ ] Cut a new release of [VRL](https://github.com/vectordotdev/vrl) if needed
- [ ] Check for any outstanding deprecation actions in [DEPRECATIONS.md](https://github.com/vectordotdev/vector/blob/master/docs/DEPRECATIONS.md) and
      take them (or have someone help you take them)
- [ ] Create a new release branch from master to freeze commits
  - `git fetch && git checkout origin/master && git checkout -b v0.<new version number> && git push -u`
- [ ] Create a new release preparation branch from `master`
  - `git checkout -b website-prepare-v0.<new version number> && git push -u`
- [ ] Pin VRL to latest released version rather than `main`
- [ ] Check if there is a newer version of [Alpine](https://alpinelinux.org/releases/) or
      [Debian](https://www.debian.org/releases/) available to update the release images in
      `distribution/docker/`. Update if so.
- [ ] Run `cargo vdev build release-cue` to generate a new cue file for the release
  - [ ] Add description key to the generated cue file with a description of the release (see
        previous releases for examples).
  - [ ] Ensure any breaking changes are highlighted in the release upgrade guide
  - [ ] Ensure any deprecations are highlighted in the release upgrade guide
  - [ ] Review generated changelog entries to ensure they are understandable to end-users
  - [ ] Copy VRL changelogs from the VRL version in the last Vector release as a new changelog entry
        ([example](https://github.com/vectordotdev/vector/blob/9c67bba358195f5018febca2f228dfcb2be794b5/website/cue/reference/releases/0.41.0.cue#L33-L64))
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
- [ ] Reset the `website` branch to the `HEAD` of the release branch to update https://vector.dev
  - [ ] `git checkout website && git reset --hard origin/v0.<new version number> && git push`
  - [ ] Confirm that the release changelog was published to https://vector.dev/releases/
    - The deployment is done by Amplify. You can see
      the [deployment logs here](https://dd-corpsite.datadoghq.com/logs?query=service%3Awebsites-vector%20branch%3Awebsite&agg_m=count&agg_m_source=base&agg_t=count&cols=host%2Cservice&fromUser=true&messageDisplay=inline&refresh_mode=sliding&storage=hot&stream_sort=time%2Casc&viz=stream).
- [ ] Release Linux packages. See [`vector-release` usage](https://github.com/DataDog/vector-release#usage).
  - Note: the pipeline inputs are the version number `v0.<new version number>` and a personal GitHub token.
  - [ ] Manually trigger the `trigger-package-release-pipeline-prod-stable` job.
- [ ] Release updated Helm chart. See [releasing Helm chart](https://github.com/vectordotdev/helm-charts#releasing).
- [ ] Once Helm chart is released, updated Vector manifests
    - Run `cargo vdev build manifests` and open a PR with changes
- [ ] Add docker images to [https://github.com/DataDog/images](https://github.com/DataDog/images/tree/master/vector) to have them available internally.
- [ ] Cherry-pick any release commits from the release branch that are not on `master`, to `master`
- [ ] Bump the release number in the `Cargo.toml` on master to the next major release
- [ ] Kick-off post-mortems for any regressions resolved by the release
