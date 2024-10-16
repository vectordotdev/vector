Previously, when the `new_relic` sink sent non-standard event fields to the logs
API, it would include those fields beside the standard event fields (i.e.
`message` and `timestamp`). Now, any such fields are sent in an `attributes`
object, as specified by the New Relic logs API documentation.
