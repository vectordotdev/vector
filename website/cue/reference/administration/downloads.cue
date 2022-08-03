package metadata

import "strings"

administration: {
	#Download: {
		#FileType: "deb" | "rpm" | "zip" | "msi" | *"tar.gz"

		#Type: "archive" | *"package"

		#Option: {
			_file_type:       #FileType
			_version_postfix: string | *""
			arch:             #Arch
			tag:              string | *strings.ToLower(arch)
			extra?:           string
			filename:         string

			if extra == _|_ {
				filename: "\(tag).\(_file_type)"
			}
			if extra != _|_ {
				filename: "\(tag)-\(extra).\(_file_type)"
			}

			// Calculate the download URL without needing site templating
			if _file_type != "deb" {
				download_url: "\(urls.vector_packages_root)/vector/{v1}/vector-{v2}-\(_version_postfix)\(filename)"
			}
			if _file_type == "deb" {
				download_url: "\(urls.vector_packages_root)/vector/{v1}/vector_{v2}-1_\(_version_postfix)\(filename)"
			}

			// Unused fields
			target:               string // The Rust compilation target
			available_on_latest:  bool | *true
			available_on_nightly: bool | *true
		}

		os:               #OperatingSystemFamily
		package_manager?: string
		title:            string | *os
		file_type:        #FileType
		type:             #Type
		version_postfix:  string | *""
		library?:         string
		options: [...{#Option & {_file_type: file_type, _version_postfix: version_postfix}}]
	}

	downloads: [#Download, ...#Download] &
		[
			{
				os:      "Linux"
				type:    "archive"
				library: "gnu"
				options: [
					{
						target: "aarch64-unknown-linux-gnu-tar-gz"
						arch:   "ARM64"
						tag:    "aarch64"
						extra:  "unknown-linux-gnu"
					},
					{
						target: "armv7-unknown-linux-gnueabihf"
						arch:   "ARMv7"
						extra:  "unknown-linux-gnueabihf"
					},
					{
						target: "x86_64-unknown-linux-gnu-tar-gz"
						arch:   "x86_64"
						extra:  "unknown-linux-gnu"
					},
				]
			},
			{
				os:              "Linux"
				title:           "Linux (deb)"
				package_manager: "DPKG"
				file_type:       "deb"
				library:         "gnu"
				options: [
					{
						target: "arm64-deb"
						arch:   "ARM64"
					},
					{
						target: "armhf-deb"
						arch:   "ARMv7"
						tag:    "armhf"
					},
					{
						target: "amd64-deb"
						arch:   "x86_64"
						tag:    "amd64"
					},
				]
			},
			{
				os:              "Linux"
				title:           "Linux (rpm)"
				package_manager: "RPM"
				file_type:       "rpm"
				version_postfix: "1."
				library:         "gnu"
				options: [
					{
						target: "aarch64-rpm"
						arch:   "ARM64"
						tag:    "aarch64"
					},
					{
						target: "armv7-rpm"
						arch:   "ARMv7"
					},
					{
						target: "x86_64-rpm"
						arch:   "x86_64"
					},
				]
			},
			{
				os:   "macOS"
				type: "archive"
				options: [
					{
						target: "x86_64-apple-darwin-tar-gz"
						arch:   "x86_64"
						extra:  "apple-darwin"
					},
				]
			},
			{
				os:        "Windows"
				file_type: "zip"
				type:      "archive"
				options: [
					{
						target: "x86_64-pc-windows-msvc-zip"
						arch:   "x86_64"
						extra:  "pc-windows-msvc"
					},
				]
			},
			{
				os:              "Windows"
				title:           "Windows (MSI)"
				package_manager: "MSI"
				file_type:       "msi"
				options: [
					{
						target: "x64-msi"
						arch:   "x86_64"
						tag:    "x64"
					},
				]
			},
		]
}
