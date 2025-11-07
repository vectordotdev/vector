The `axiom` sink now supports regional edges for data locality. A new optional `region` configuration field allows you to specify the regional edge domain (e.g., `eu-central-1.aws.edge.axiom.co`). When configured, data is sent to `https://{region}/v1/ingest/{dataset}`. The `url` field now intelligently handles paths: URLs with custom paths are used as-is, while URLs without paths maintain backwards compatibility by appending `/v1/datasets/{dataset}/ingest`.

authors: toppercodes
