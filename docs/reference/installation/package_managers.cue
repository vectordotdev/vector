package metadata

installation: package_managers: {
	apt: {
		title:       "APT"
		description: installation._interfaces.apt.description
	}

	dpkg: {
		title:       "DPKG"
		description: installation._interfaces.dpkg.description
	}

	helm: {
		title:       "Helm"
		description: installation._interfaces."helm3".description
	}

	homebrew: {
		title:       "Homebrew"
		description: installation._interfaces.homebrew.description
	}

	msi: {
		title:       "MSI"
		description: installation._interfaces.msi.description
	}

	nix: {
		title:       "Nix"
		description: installation._interfaces.nix.description
	}

	rpm: {
		title:       "RPM"
		description: installation._interfaces.rpm.description
	}

	yum: {
		title:       "YUM"
		description: installation._interfaces.yum.description
	}
}
