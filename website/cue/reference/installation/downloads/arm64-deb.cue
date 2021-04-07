package metadata

installation: downloads: "arm64-deb": {
	available_on_latest:  true
	available_on_nightly: true
	arch:                 "ARM64"
	file_name:            "vector-{version}-arm64.deb"
	file_type:            "deb"
	library:              "gnu"
	os:                   "Linux"
	package_manager:      installation.package_managers.dpkg.name
	type:                 "package"
}
