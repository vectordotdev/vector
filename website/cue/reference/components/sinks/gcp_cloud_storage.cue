package metadata

components: sinks: gcp_cloud_storage: {
	title: "GCP Cloud Storage (GCS)"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["GCP"]
		stateful: false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10_000_000
				timeout_secs: 300.0
			}
			compression: {
				enabled: true
				default: "none"
				algorithms: ["gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					framing: true
					enum: ["json", "text"]
				}
			}
			proxy: enabled: true
			request: {
				enabled:        true
				rate_limit_num: 1000
				headers:        false
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
			to: {
				service: services.gcp_cloud_storage

				interface: {
					socket: {
						api: {
							title: "GCP XML Interface"
							url:   urls.gcp_xml_interface
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: base.components.sinks.gcp_cloud_storage.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		object_access_control_list: {
			title: "Object access control list (ACL)"
			body:  """
					GCP Cloud Storage supports access control lists (ACL) for buckets and
					objects. In the context of Vector, only object ACLs are relevant (Vector
					does not create or modify buckets). You can set the object level ACL by
					using the `acl` option, which allows you to set one of the [predefined
					ACLs](\(urls.gcs_predefined_acl)) on each created object.
					"""
		}
		object_naming: {
			title: "Object naming"
			body:  """
				By default, Vector names your GCS objects in accordance with one of two formats.

				If compression *is* enabled, this format is used:

				```text
				key_prefix><timestamp>-<uuidv4>.log.gz
				```

				Here's an example name in the compression-enabled format:

				```text
				date=2019-06-18/1560886634-fddd7a0e-fad9-4f7e-9bce-00ae5debc563.log.gz
				```

				If compression is *not* enabled, this format is used:

				```text
				<key_prefix><timestamp>-<uuidv4>.log
				```

				Here's an example name in the compression-disabled format:

				```text
				date=2019-06-18/1560886634-fddd7a0e-fad9-4f7e-9bce-00ae5debc563.log
				```

				Vector appends a [UUIDV4](\(urls.uuidv4)) token to ensure there are no name
				conflicts in the unlikely event that two Vector instances are writing data at the same
				time.

				You can control the resulting name via the [`key_prefix`](#key_prefix),
				[`filename_time_format`](#filename_time_format),
				and [`filename_append_uuid`](#filename_append_uuid) options.

				For example, to store objects at the root GCS folder, without a timestamp or UUID use
				these configuration options:

				```text
				key_prefix = "{{ my_file_name }}"
				filename_time_format = ""
				filename_append_uuid = false
				```
				"""
		}

		storage_class: {
			title: "Storage Class"
			body:  """
					GCS offers [storage classes](\(urls.gcs_storage_classes)). You can apply
					defaults, and rules, at the bucket level or set the storage class at the
					object level. In the context of Vector only the object level is relevant
					(Vector does not create or modify buckets). You can set the storage
					class via the `storage_class` option.
					"""
		}

		tags_and_metadata: {
			title: "Tags & Metadata"
			body:  """
					Vector supports adding [custom metadata](\(urls.gcs_custom_metadata)) to
					created objects. These metadata items are a way of associating extra
					data items with the object that are not part of the uploaded data.
					"""
		}
	}

	permissions: iam: [
		{
			platform: "gcp"
			_service: "storage"

			policies: [
				{
					_action: "objects.create"
					required_for: ["operation"]
				},
				{
					_action: "objects.get"
					required_for: ["healthcheck"]
				},
			]
		},
	]
}
