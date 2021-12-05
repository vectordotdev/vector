package metadata

services: nginx: {
	name:     "Nginx"
	thing:    "an \(name) server"
	url:      urls.nginx
	versions: null

	description: "[Nginx](\(urls.nginx)) is an HTTP and reverse proxy server, a mail proxy server, and a generic TCP/UDP proxy server."
}
