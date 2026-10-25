`--watch-config` now keeps detecting changes to files referenced by components, such as enrichment table files, when they are reached through a symlink that is atomically repointed (for example a Kubernetes ConfigMap volume or git-sync checkout). Previously, only the first change was detected because the resolved target of the symlink was watched instead of the configured path.

authors: iShiBin
