package metadata

components: sources: azure_blob: {
	title: "Azure Blob Storage"

	features: {
		auto_generated:   true
		acknowledgements: true
		multiline: enabled: true
		collect: {
			tls: enabled:        false
			checkpoint: enabled: false
			proxy: enabled:      true
			from: service:       services.azure_blob
		}
	}

	classes: {
		deployment_roles: ["aggregator"]
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	support: {
		requirements: [
			"""
				The Azure Blob Storage source requires an Azure Storage Queue that
				receives `Microsoft.Storage.BlobCreated` notifications from an
				[Event Grid subscription](\(urls.azure_event_grid_blob)) on the
				storage account.
				""",
		]
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: generated.components.sources.azure_blob.configuration

	output: {
		logs: object: {
			description: "A line from a blob in Azure Blob Storage."
			fields: {
				message: {
					description: "A line from the blob."
					required:    true
					type: string: {
						examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
					}
				}
				timestamp: fields._current_timestamp & {
					description: "The Last-Modified time of the blob, falling back to the notification's event time. Defaults to the current timestamp if this information is missing."
				}
				source_type: {
					description: "The name of the source type."
					required:    true
					type: string: {
						examples: ["azure_blob"]
					}
				}
				container: {
					description: "The container of the blob the line came from."
					required:    true
					type: string: {
						examples: ["insights-logs"]
					}
				}
				blob: {
					description: "The blob the line came from."
					required:    true
					type: string: {
						examples: ["resourceId=/SUBSCRIPTIONS/.../y=2026/m=06/d=01/h=12/m=00/PT1H.json"]
					}
				}
				storage_account: {
					description: "The storage account of the blob the line came from, when it can be determined from the notification."
					required:    false
					common:      true
					type: string: {
						default: null
						examples: ["mylogstorage"]
					}
				}
			}
		}
	}

	how_it_works: {
		setup: {
			title: "Blob discovery through Event Grid"
			body:  """
				This source does not scan the storage account for blobs. Instead, it relies
				on [Azure Event Grid](\(urls.azure_event_grid_blob)) publishing a
				notification to an [Azure Storage Queue](\(urls.azure_storage_queue))
				whenever a blob is created:

				1. Create a Storage Queue in the storage account (or another account).
				2. Create an Event Grid subscription on the storage account, filtered to
				   the `Microsoft.Storage.BlobCreated` event type, with the Storage Queue
				   as its endpoint.

				Vector polls the queue, downloads each newly created blob, and deletes the
				queue message once the events have been delivered. Notifications for other
				event types are ignored and deleted from the queue. Both the Event Grid
				and CloudEvents 1.0 schemas are supported and detected automatically.

				Because a queue message must only be processed (and deleted) once, each
				Vector instance must consume its own Storage Queue. To send the same
				events to multiple destinations, configure multiple sinks on the same
				source instead of multiple sources sharing a queue.
				"""
		}
		events: {
			title: "Handling events from the `azure_blob` source"
			body:  """
				This source behaves very similarly to the `file` source in that
				it outputs one event per line (unless the `multiline`
				configuration option is used), and you will commonly want to use
				[transforms](\(urls.vector_transforms)) to parse the data.
				"""
		}
		failed_messages: {
			title: "Failed message handling"
			body: """
				When a blob referenced by a queue message cannot be fetched or read, the
				message is left in the queue and becomes visible again after
				`queue.visibility_timeout_secs`, so ingestion is retried. Azure Storage
				Queues have no dead-letter queue: a message that fails permanently is
				redelivered indefinitely. The `dequeue_count` field in the error log makes
				such poison messages observable.
				"""
		}
	}

	telemetry: metrics: {
		azure_blob_event_ignored_total:                   components.sources.internal_metrics.output.metrics.azure_blob_event_ignored_total
		azure_blob_processing_failed_duration_seconds:    components.sources.internal_metrics.output.metrics.azure_blob_processing_failed_duration_seconds
		azure_blob_processing_succeeded_duration_seconds: components.sources.internal_metrics.output.metrics.azure_blob_processing_succeeded_duration_seconds
		azure_queue_message_delete_succeeded_total:       components.sources.internal_metrics.output.metrics.azure_queue_message_delete_succeeded_total
		azure_queue_message_processing_succeeded_total:   components.sources.internal_metrics.output.metrics.azure_queue_message_processing_succeeded_total
		azure_queue_message_receive_succeeded_total:      components.sources.internal_metrics.output.metrics.azure_queue_message_receive_succeeded_total
		azure_queue_message_received_messages_total:      components.sources.internal_metrics.output.metrics.azure_queue_message_received_messages_total
	}
}
