The Debian package now ships `/etc/vector/vector.yaml` as a dpkg conffile (a stub with no active sources or sinks), so local modifications to it are preserved across package upgrades instead of being unknown to the package manager.

Because the path was not previously a conffile, a file already present there is moved aside before the new conffile is unpacked and restored immediately afterwards, byte-for-byte. Upgrades therefore complete without the interactive "file created by you" conffile prompt that would otherwise break unattended `apt` and `dpkg` runs.

authors: koenserry
