package metadata

installation: downloads: "armhf-deb": {
	available_on_latest:  true
	available_on_nightly: true
	arch:                 "ARMv7"
	file_name:            "vector-{version}-armhf.deb"
	file_type:            "deb"
	library:              "gnu"
	os:                   "Linux"
	package_manager:      installation.package_managers.dpkg.name
	type:                 "package"
}
