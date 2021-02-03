package metadata

import "strconv"

remap: {
	#Error: {
		anchor:      "\(code)"
		code:        >=100 & <1000 & int
		description: string
		rationale:   string | null
		resolution:  string
		title:       string

		examples: [remap.#Example, ...remap.#Example]
	}

	errors: [Code=string]: #Error & {
		code: strconv.ParseInt(Code, 0, 8)
	}
}
