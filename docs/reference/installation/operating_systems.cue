package metadata

installation: operating_systems: {
	"amazon-linux": {
		interfaces: [
			installation._interfaces.yum,
			installation._interfaces.rpm,
			installation._interfaces."vector-cli" & {
				roles: agent: commands: variables: config: sources: in: type: components.sources.journald.type
			},
			installation._interfaces."docker-cli",
		]
		os:    "Linux"
		title: "Amazon Linux"
	}

	centos: {
		interfaces: [
			installation._interfaces.yum,
			installation._interfaces.rpm,
			installation._interfaces."vector-cli" & {
				roles: agent: commands: variables: config: sources: in: type: components.sources.journald.type
			},
			installation._interfaces."docker-cli",
		]
		os:    "Linux"
		title: "CentOS"
	}

	debian: {
		interfaces: [
			installation._interfaces.apt,
			installation._interfaces.dpkg,
			installation._interfaces."vector-cli" & {
				roles: agent: commands: variables: config: sources: in: type: components.sources.journald.type
			},
			installation._interfaces."docker-cli",
		]
		os:    "Linux"
		title: "Debian"
	}

	macos: {
		interfaces: [
			installation._interfaces.homebrew & {
				roles: agent: commands: variables: config: sources: in: {
					type: components.sources.file.type
					include: ["/var/log/system.log"]
				}
			},
			installation._interfaces."vector-cli" & {
				roles: agent: commands: variables: config: sources: in: {
					type: components.sources.file.type
					include: ["/var/log/system.log"]
				}
			},
			installation._interfaces."docker-cli",
		]
		os:    "Linux"
		title: "MacOS"
	}

	nixos: {
		interfaces: [
			installation._interfaces.nix,
			installation._interfaces."vector-cli" & {
				roles: agent: commands: variables: config: sources: in: type: components.sources.journald.type
			},
			installation._interfaces."docker-cli",
		]
		os:    "Linux"
		title: "NixOS"
	}

	raspbian: {
		interfaces: [
			installation._interfaces."vector-cli" & {
				roles: agent: commands: variables: config: sources: in: type: components.sources.journald.type
			},
			installation._interfaces."docker-cli",
		]
		os:    "Linux"
		title: "Raspbian"
	}

	rhel: {
		interfaces: [
			installation._interfaces.yum,
			installation._interfaces.rpm,
			installation._interfaces."vector-cli" & {
				roles: agent: commands: variables: config: sources: in: type: components.sources.journald.type
			},
			installation._interfaces."docker-cli",
		]
		os:    "Linux"
		title: "RHEL"
	}

	ubuntu: {
		interfaces: [
			installation._interfaces.apt,
			installation._interfaces.dpkg,
			installation._interfaces."vector-cli" & {
				roles: agent: commands: variables: config: sources: in: type: components.sources.journald.type
			},
			installation._interfaces."docker-cli",
		]
		os:    "Linux"
		title: "Ubuntu"
	}

	windows: {
		interfaces: [
			installation._interfaces.msi,
			installation._interfaces."vector-cli" & {
				roles: agent: commands: variables: config: sources: in: type: components.sources.host_metrics.type
			},
			installation._interfaces."docker-cli",
		]
		os:    "Windows"
		title: "Windows"
	}
}
