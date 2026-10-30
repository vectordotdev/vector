# Vector debugging guide files

Follow the [debugging guide](https://vector.dev/guides/developer/debugging/) for the walkthrough and explanations.

| File | Purpose |
| --- | --- |
| `http-server.py` | Local HTTP server used to simulate failures. |
| `vector-start.yaml` | Starting pipeline. Edit this file during the scenarios. |
| `vector-final.yaml` | Completed pipeline with zlib compression and smaller batches. |
| `vector-datadog.yaml` | Final pipeline with internal metrics sent to Datadog. |
| `vector-prometheus.yaml` | Final pipeline with internal metrics exposed for Prometheus. |
| `prometheus.yaml` | Prometheus scrape configuration. |
| `compose.yaml` | Prometheus and Grafana services. |
| `README.md` | Quick start and file reference. |

## Quick start

Install [Vector](https://vector.dev/docs/setup/installation/) and [Python](https://www.python.org/downloads/).
The walkthrough also uses `curl`. Run commands from this folder in each terminal, with ports 8000 and 8686 available.

Start the HTTP server in one terminal:

```shell
python3 http-server.py
```

Start Vector in another terminal:

```shell
vector --watch-config --config vector-start.yaml
```

Edit `vector-start.yaml` as you work through the guide. Vector reloads it when you save changes.
Use `vector-final.yaml` to try the completed configuration. Stop the running Vector process before switching files.

## Monitoring

Keep the HTTP server running for either setup.

For Datadog, set `DATADOG_API_KEY` in Vector's environment and set the sink's `site` option if needed, then run:

```shell
vector --watch-config --config vector-datadog.yaml
```

For Prometheus and Grafana, run:

```shell
vector --watch-config --config vector-prometheus.yaml
```

In another terminal, start the monitoring services with Docker Compose:

```shell
docker compose up -d
```

Open [Grafana](http://localhost:3000) with username `admin` and password `admin`.
Add a Prometheus data source with the URL `http://prometheus:9090`.

Stop Vector and the HTTP server with Ctrl+C. Stop the monitoring services with `docker compose down`.
