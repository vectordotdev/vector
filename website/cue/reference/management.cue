package metadata

import "strings"

management: {
	_root_url: "https://packages.timber.io"

	#Arch: "ARM64" | "ARMv7" | "x86_64"
	#OS: "Linux" | "macOS" | "Windows"

	#Download: {
		#Option: {
			_file_type: string
			arch: #Arch
			tag: string | *strings.ToLower(arch)
			extra?: string
			filename: string
			_version_postfix: string | *""

			if extra == _|_ {
				filename: "\(tag).\(_file_type)"
			}
			if extra != _|_ {
				filename: "\(tag)-\(extra).\(_file_type)"
			}

			download_url: "\(_root_url)/vector/{version}/vector-{version}\(_version_postfix)-\(filename)"
		}

		os: #OS
		title: string | *os
		file_type: string | *"tar.gz"
		version_postfix: string | *""
		options: [...#Option & { _file_type: file_type, _version_postfix: version_postfix }]
	}

	downloads: [#Download, ...#Download] & [
		{
			os: "Linux"
			options: [
				{
					arch: "ARM64"
					tag: "aarch64"
					extra: "unknown-linux-gnu"
				},
				{
					arch: "ARMv7"
					extra: "unknown-linux-gnueabihf"
				},
				{
					arch: "x86_64"
				}
			]
		},
		{
			os: "Linux"
			title: "Linux (deb)"
			file_type: "deb"
			options: [
				{
					arch: "ARM64"
				},
				{
					arch: "ARMv7"
					tag: "armhf"
				},
				{
					arch: "x86_64"
					tag: "amd64"
				}
			]
		},
		{
			os: "Linux"
			title: "Linux (rpm)"
			file_type: "rpm"
			version_postfix: "-1"
			options: [
				{
					arch: "ARM64"
					tag: "aarch64"
				},
				{
					arch: "ARMv7"
				},
				{
					arch: "x86_64"
				}
			]
		},
		{
			os: "macOS"
			options: [
				{
					arch: "x86_64"
					extra: "apple-darwin"
				}
			]
		},
		{
			os: "Windows"
			file_type: "zip"
			options: [
				{
					arch: "x86_64"
					extra: "pc-windows-msvc"
				}
			]
		},
		{
			os: "Windows"
			file_type: "msi"
			options: [
				{
					arch: "x86_64"
					tag: "x64"
				}
			]
		}
	]
}
