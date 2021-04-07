package metadata

installation: downloads: "aarch64-rpm": {
	available_on_latest:  true
	available_on_nightly: true
	arch:                 "ARM64"
	file_name:            "vector-{version}-1.aarch64.rpm"
	file_type:            "rpm"
	library:              "gnu"
	os:                   "Linux"
	package_manager:      installation.package_managers.rpm.name
	type:                 "package"
}
