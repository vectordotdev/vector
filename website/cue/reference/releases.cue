package metadata

releases: {
	#SemanticType: "chore" | "docs" | "enhancement" | "feat" | "fix" | "perf" | "status" | "deprecation"

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
		sha:  #CommitSha
		type: #SemanticType
	}

	#CommitSha: =~"^[a-z0-9]{40}$"

	#ChangeLogEntry: {
		type: #SemanticType
		scopes: [string, ...string] | *[]
		breaking:    bool | *false
		description: string
		pr_numbers: [uint, ...uint] | *[]
		contributors: [string, ...string] | *[]
	}

	#Release: {
		version:      string
		codename?:    string
		date:         string
		description?: string
		known_issues: [string, ...string] | *[]

		commits?: [#Commit, ...#Commit]
		changelog: [#ChangeLogEntry, ...#ChangeLogEntry] | *[]
		whats_next: #Any | *[]
	}

	{[Version=string]: #Release & {version: Version}}
}
