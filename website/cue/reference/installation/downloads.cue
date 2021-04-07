package metadata

installation: {
	#Download: {
		available_on_latest:  bool
		available_on_nightly: bool
		arch:                 #Arch
		file_name:            string
		file_type:            string
		library:              string | null
		name:                 string
		os:                   #OperatingSystemFamily
		package_manager?:     string
		title:                "\(os) (\(arch))"
		type:                 "archive" | "package"
	}

	#Downloads: [Name=string]: #Download & {
		name: Name
	}

	downloads: #Downloads
}
