package metadata

installation: downloads: "armv7-rpm": {
	available_on_latest:  true
	available_on_nightly: true
	arch:                 "ARMv7"
	file_name:            "vector-{version}-1.armv7.rpm"
	file_type:            "rpm"
	library:              "gnu"
	os:                   "Linux"
	package_manager:      installation.package_managers.rpm.name
	type:                 "package"
}
