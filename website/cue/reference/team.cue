package metadata

import "strings"

#Member: {
	id:        strings.ToLower(name)
	name:      !=""
	active:    bool | *true
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
		name:     "Ben Johnson"
		_github:  "binarylogic"
		_keybase: "binarylogic"
		_twitter: "binarylogic"
		bio: """
			Ben is an engineering manager at Datadog managing the Vector project (formerly the CTO/Co-Founder at Timber.io).
			"""
	},
	{
		name:    "Brian L. Troutwine"
		_github: "blt"
		bio: """
			Brian is a staff engineer at Datadog and a member of the Vector team.
			"""
	},
	{
		name:    "Bruce Guenter"
		_github: "bruceg"
		bio: """
			Bruce is a software engineer at Datadog on the Vector project (formerly part of Timber.io).
			"""
	},
	{
		name:     "James Turnbull"
		_github:  "jamtur01"
		_keybase: "jamtur01"
		_twitter: "kartar"
		bio: """
			James was the VP of Engineering at Timber.io.
			"""
		active: false
	},
	{
		name:     "Jean Mertz"
		_github:  "JeanMertz"
		_keybase: "JeanMertz"
		_twitter: "JeanMertz"
		bio: """
			Jean is a senior engineer at Datadog (formerly part of Timber.io).
			"""
	},
	{
		name:     "Jesse Szwedko"
		_github:  "jszwedko"
		_keybase: "jszwedko"
		_twitter: "jszwedko"
		bio: """
			Jesse is an engineer at Datadog (formerly Timber.io).
			"""
	},
	{
		name:     "Kirill Fomichev"
		_github:  "fanatid"
		_keybase: "fanatid"
		bio: """
			Kirill was an engineer at Timber.io.
			"""
		active: false
	},
	{
		name:    "Kruno Tomola Fabro"
		_github: "ktff"
		bio: """
			Kruno is a contractor for Datadog on the Vector project (formerly contracted with at Timber.io).
			"""
	},
	{
		name:    "Lee Benson"
		_github: "leebenson"
		bio: """
			Lee is a senior engineer at Datadog (formerly part of Timber.io).
			"""
	},
	{
		name:     "Luc Perkins"
		_github:  "lucperkins"
		_keybase: "lucperkins"
		_twitter: "lucperkins"
		bio: """
			Luc is a technical writer at Datadog (formerly part of Timber.io).
			"""
	},
	{
		name:     "Luke Steensen"
		_github:  "lukesteensen"
		_keybase: "lukesteensen"
		_twitter: "lukesteensen"
		bio: """
			Luke is the team lead of the Vector project at Datadog (formerly part of Timber.io).
			Before Timber, Luke was an engineer at Braintree working on parts of their
			observability infrastructure.
			"""
	},
	{
		name:     "Mike"
		_github:  "MOZGIII"
		_keybase: "MOZGIII"
		_twitter: "MOZGIII"
		bio: """
			Mike was a senior engineer at Timber.io.
			"""
		active: false
	},
	{
		name:    "Spencer Gilbert"
		_github: "spencergilbert"
		bio: """
			Spencer is an engineer at Datadog.
			"""
	},
	{
		name:     "Steve Hall"
		_github:  "sghall"
		_keybase: "sghall"
		_twitter: "sghall"
		bio: """
			Steve is an engineer at Datadog (formerly part of Timber.io).
			"""
	},
	{
		name:    "Vic"
		_github: "vector-vic"
		bio: """
			Vic is the Vector mascot.
			"""
	},
	{
		name:     "Zach Sherman"
		_github:  "zsherman"
		_keybase: "zsherman"
		_twitter: "zsherman"
		bio: """
			Zach is the product manager of the Vector project at Datadog (formerly the CEO/co-founder of Timber.io).
			"""
	},
]
