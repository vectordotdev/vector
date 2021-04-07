package metadata

installation: downloads: "x64-msi": {
	available_on_latest:  true
	available_on_nightly: true
	arch:                 "x86_64"
	file_name:            "vector-{version}-x64.msi"
	file_type:            "msi"
	library:              null
	os:                   "Windows"
	package_manager:      installation.package_managers.msi.name
	type:                 "package"
}
