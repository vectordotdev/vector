package metadata

releases: {
	#SemanticType: "chore" | "docs" | "enhancement" | "feat" | "fix" | "perf" | "security" | "status" | "revert"

	#Commit: {
		author:           string
		breaking_change:  bool
		date:             #Date
		description:      string
		deletions_count:  uint
		files_count:      uint
		insertions_count: uint
		pr_number:        uint | null
		scopes: [string, ...string] | *[]
		sha:   #CommitSha
		type?: string
	}

	#CommitSha: =~"^[a-z0-9]{40}$"

	#ChangeLogEntry: {
		type: #SemanticType
		scopes: [string, ...string] | *[]
		breaking:    bool | *false
		title?:      string
		anchor?:     string
		description: string
		pr_numbers: [uint, ...uint] | *[]
		contributors: [string, ...string] | *[]
	}

	#DeprecationEntry: {
		what:             string
		deprecated_since: string
		description:      string
	}

	#EnactedDeprecationEntry: {
		#DeprecationEntry
		removed_in: string
	}

	#Release: {
		version:      string
		date:         string
		description?: string
		known_issues: [string, ...string] | *[]

		commits?: [#Commit, ...#Commit]
		changelog: [#ChangeLogEntry, ...#ChangeLogEntry] | *[]
		vrl_changelog?: string
		whats_next?: [...{title: string, description: string}]
	}

	{[Version=string]: #Release & {version: Version}}
}
