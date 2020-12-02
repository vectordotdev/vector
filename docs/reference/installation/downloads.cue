package metadata

installation: downloads: {
	"x86_64-unknown-linux-musl-tar-gz": {
		available_on_latest:  true
		available_on_nightly: true
		arch:                 "x86_64"
		file_name:            "vector-{version}-x86_64-unknown-linux-musl.tar.gz"
		file_type:            "tar.gz"
		library:              "musl"
		os:                   "Linux"
		type:                 "archive"
	}

	"aarch64-unknown-linux-musl-tar-gz": {
		available_on_latest:  true
		available_on_nightly: true
		arch:                 "ARM64"
		file_name:            "vector-{version}-aarch64-unknown-linux-musl.tar.gz"
		file_type:            "tar.gz"
		library:              "musl"
		os:                   "Linux"
		type:                 "archive"
	}

	"armv7-unknown-linux-gnueabihf-tar-gz": {
		available_on_latest:  true
		available_on_nightly: true
		arch:                 "ARMv7"
		file_name:            "vector-{version}-armv7-unknown-linux-gnueabihf.tar.gz"
		file_type:            "tar.gz"
		library:              "gnu"
		os:                   "Linux"
		type:                 "archive"
	}

	"armv7-unknown-linux-musleabihf-tar-gz": {
		available_on_latest:  true
		available_on_nightly: true
		arch:                 "ARMv7"
		file_name:            "vector-{version}-armv7-unknown-linux-musleabihf.tar.gz"
		file_type:            "tar.gz"
		library:              "musl"
		os:                   "Linux"
		type:                 "archive"
	}

	"x86_64-apple-darwin-tar-gz": {
		available_on_latest:  true
		available_on_nightly: true
		arch:                 "x86_64"
		file_name:            "vector-{version}-x86_64-apple-darwin.tar.gz"
		file_type:            "tar.gz"
		library:              null
		os:                   "macOS"
		type:                 "archive"
	}

	"x86_64-pc-windows-msvc-zip": {
		available_on_latest:  true
		available_on_nightly: true
		arch:                 "x86_64"
		file_name:            "vector-{version}-x86_64-pc-windows-msvc.zip"
		file_type:            "zip"
		library:              null
		os:                   "Windows"
		type:                 "archive"
	}

	"x64-msi": {
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

	"amd64-deb": {
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

	"arm64-deb": {
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

	"armhf-deb": {
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

	"x86_64-rpm": {
		available_on_latest:  true
		available_on_nightly: true
		arch:                 "x86_64"
		file_name:            "vector-{version}-1.x86_64.rpm"
		file_type:            "rpm"
		library:              "gnu"
		os:                   "Linux"
		package_manager:      installation.package_managers.rpm.name
		type:                 "package"
	}

	"aarch64-rpm": {
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

	"armv7-rpm": {
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
}
