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
		name:     "Ben"
		_github:  "binarylogic"
		_keybase: "binarylogic"
		_twitter: "binarylogic"
		bio: """
			Ben is an engineering manager at Datadog mananging the Vector project (formerly the CTO/Co-Founder at Timber.io).
			"""
	},
	{
		name:    "Bruce"
		_github: "bruceg"
		bio: """
			Bruce is a software engineer at Datadog on the Vector project (formely part of Timber.io).
			"""
	},
	{
		name:     "James"
		_github:  "jamtur01"
		_keybase: "jamtur01"
		_twitter: "kartar"
		bio: """
			James was the VP of Engineering at Timber.io.
			"""
	},
	{
		name:     "Jean"
		_github:  "JeanMertz"
		_keybase: "JeanMertz"
		_twitter: "JeanMertz"
		bio: """
			Jean is a senior engineer at Datadog (formerly part of Timber.io).
			"""
	},
	{
		name:     "Jesse"
		_github:  "jszwedko"
		_keybase: "jszwedko"
		_twitter: "jszwedko"
		bio: """
			Jesse is an engineer at Datadog (formerly Timber.io).
			"""
	},
	{
		name:     "Kirill"
		_github:  "fanatid"
		_keybase: "fanatid"
		bio: """
			Kirill was is an engineer at Timber.io.
			"""
	},
	{
		name:    "Kruno"
		_github: "ktff"
		bio: """
			Kruno is an engineer at Timber.io and a member of the Vector team.
			"""
	},
	{
		name:     "Lee"
		_github:  "leebenson"
		_keybase: "leebenson"
		_twitter: "leebenson"
		bio: """
			Lee is a senior engineer at Datadog (formerly part of Timber.io).
			"""
	},
	{
		name:     "Luc"
		_github:  "lucperkins"
		_keybase: "lucperkins"
		_twitter: "lucperkins"
		bio: """
			Luc is a technical writer at Datadog (formerly part of Timber.io).
			"""
	},
	{
		name:     "Luke"
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
	},
	{
		name:     "Steve"
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
		name:     "Zach"
		_github:  "zsherman"
		_keybase: "zsherman"
		_twitter: "zsherman"
		bio: """
			Zach is the product manager of the Vector project at Datadog (formerly the CEO/co-founder of Timber.io).
			"""
	},
]
