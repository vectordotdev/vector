package metadata

services: amqp: {
	name:     "AMQP"
	thing:    "\(name) topics"
	url:      urls.amqp_protocol
	versions: "= 0.9"

	description: "The Advanced Message Queuing Protocol (AMQP) is an open standard for passing business messages between applications or organizations.  It connects systems, feeds business processes with the information they need and reliably transmits onward the instructions that achieve their goals."
}
