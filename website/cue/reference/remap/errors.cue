package metadata

import "strconv"

remap: {
	#Error: {
		anchor:      "\(code)"
		code:        >=100 & <1000 & int
		title:       string
		description: string
		rationale:   string | *null
		resolution?: string

		examples?: [remap.#Example, ...remap.#Example]
	}

	errors: [Code=string]: #Error & {
		code: strconv.ParseInt(Code, 0, 16)
	}

	_fail_safe_blurb: """
		VRL is [fail safe](\(urls.vrl_fail_safety)) and thus requires that all possible runtime errors be handled.
		This provides important [safety guarantees](\(urls.vrl_safety)) to VRL and helps to ensure that VRL programs
		run reliably when deployed.
		"""
}
