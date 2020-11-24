package metadata

import "strings"

#Member: {
	id:        strings.ToLower(name)
	name:      !=""
	_github:   !=""
	_twitter?: !=""
	_keybase?: !=""
	avatar:    "\(github).png"
	bio?:      !=""
	github:    "https://github.com/\(_github)"

	if _twitter != _|_ {
		twitter: "https://twitter.com/\(_twitter)"
	}

	if _keybase != _|_ {
		keybase: "https://keybase.io/\(_keybase)"
	}
}

team: [#Member, ...#Member] & [
	{
		name:     "Ana"
		_github:  "hoverbear"
		_twitter: "a_hoverbear"
		bio:      """
			Ana is the proprietor of [Hoverbear Consulting](https://hoverbear.org)
			and [Timber.io](\(urls.timber)) supports her work as a [core Vector
			team](\(urls.team)) member. She is focused on building capacity within
			the Rust ecosystem and her local community. A frequent speaker, teacher,
			and mentor, she was a founding organizer of [RustFest](https://rustfest.eu),
			[RustCon Asia](https://rustcon.asia), and [Rust Belt
			Rust](https://www.rust-belt-rust.com/).
			"""
	},
	{
		name:     "Ben"
		_github:  "binarylogic"
		_keybase: "binarylogic"
		_twitter: "binarylogic"
		bio:      """
			Ben is the CTO/Co-Founder at [Timber](\(urls.timber)) and a member of the
			[core Vector team](\(urls.team)). He is an open-source veteran, creating
			[Authlogic](https://github.com/binarylogic/authlogi) over 15 years ago
			before helping to launch Vector.
			"""
	},
	{
		name:    "Bruce"
		_github: "bruceg"
	},
	{
		name:     "James"
		_github:  "jamtur01"
		_keybase: "jamtur01"
		_twitter: "kartar"
	},
	{
		name:     "Jean"
		_github:  "JeanMertz"
		_keybase: "JeanMertz"
		_twitter: "JeanMertz"
	},
	{
		name:     "Jesse"
		_github:  "jszwedko"
		_keybase: "jszwedko"
		_twitter: "jszwedko"
	},
	{
		name:     "Kirill"
		_github:  "fanatid"
		_keybase: "fanatid"
	},
	{
		name:    "Kruno"
		_github: "ktff"
	},
	{
		name:     "Lee"
		_github:  "leebenson"
		_keybase: "leebenson"
		_twitter: "leebenson"
	},
	{
		name:     "Luc"
		_github:  "lucperkins"
		_keybase: "lucperkins"
		_twitter: "lucperkins"
	},
	{
		name:     "Luke"
		_github:  "lukesteensen"
		_keybase: "lukesteensen"
		_twitter: "lukesteensen"
		bio:      """
			Luke is a Senior Engineer at [Timber.io](\(urls.timber)) and a
			member of the [core Vector team](\(urls.team)). Before Timber,
			Luke was an engineer at Braintree working on parts of their
			observability infrastructure.
			"""
	},
	{
		name:     "Mike"
		_github:  "MOZGIII"
		_keybase: "MOZGIII"
		_twitter: "MOZGIII"
	},
	{
		name:     "Steve"
		_github:  "sghall"
		_keybase: "sghall"
		_twitter: "sghall"
	},
	{
		name:    "Vic"
		_github: "vector-vic"
	},
	{
		name:     "Zach"
		_github:  "zsherman"
		_keybase: "zsherman"
		_twitter: "zsherman"
	},
]
