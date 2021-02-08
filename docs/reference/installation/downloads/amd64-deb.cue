package metadata

installation: downloads: "amd64-deb": {
	available_on_latest:  true
	available_on_nightly: true
	arch:                 "x86_64"
	file_name:            "vector-{version}-amd64.deb"
	file_type:            "deb"
	library:              "gnu"
	os:                   "Linux"
	package_manager:      installation.package_managers.dpkg.name
	type:                 "package"
}
