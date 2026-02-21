package metadata

services: azure_event_hubs: {
	name:     "Azure Event Hubs"
	thing:    "\(name) event hubs"
	url:      urls.azure_event_hubs
	versions: null

	description: "[Azure Event Hubs](\(urls.azure_event_hubs)) is a fully managed, real-time data ingestion service from Microsoft Azure. It can receive and process millions of events per second with low latency, serving as the front door for an event pipeline."
}
