package metadata

remap: errors: "305": {
	title: "Divide by zero"
	description: """
		You've attempted to divide an integer or float by zero.
		"""
	rationale: """
		Unlike some other programming languages, VRL doesn't have any concept of infinity, as it's
		unclear how that could be germane to observability data use cases. Thus, dividing by zero
		can't have any meaningful result.
		"""
	resolution: """
		If you know that a value is necessarily zero, don't divide by it. If a value *could* be
		zero, capture the potential error thrown by the operation:

		```coffee
		result, err = 27 / .some_value
		if err != nil {
			# Handle error
		}
		```
		"""
}
