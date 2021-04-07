package metadata

releases: {
	#Commit: {
		author:           string
		breaking_change:  bool
		date:             #Date
		description:      string
		deletions_count:  uint
		files_count:      uint
		insertions_count: uint
		pr_number:        uint | null
		scopes:           [string, ...string] | *[]
		sha:              #CommitSha
		type:             "chore" | "docs" | "enhancement" | "feat" | "fix" | "perf" | "status"
	}

	#CommitSha: =~"^[a-z0-9]{40}$"

	#Release: {
		codename:     string
		date:         string
		description?: string

		commits: [#Commit, ...#Commit]
		whats_next: #Any
	}

	{[Name=string]: #Release}
}
