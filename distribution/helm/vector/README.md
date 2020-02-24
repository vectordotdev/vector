# Vector

[Vector](https://vector.dev) is used to ship Kubernetes and host logs to multiple outputs.

## Prerequisites

- Kubernetes 1.10+

## Note

By default this chart only ships a single output to container logs.

## Installing the Chart

To install the chart with the release name `vector`:

```bash
$ helm install --name vector stable/vector
```

## Configuration

The following table lists the configurable parameters of the vector chart and their default values.

| Parameter                       | Description                                                                            | Default           |
| ------------------------------- | -------------------------------------------------------------------------------------- | ----------------- |
| `image.repository`              | Docker image repo                                                                      | `timberio/vector` |
| `image.tag`                     | Docker image tag                                                                       | `latest-alpine`   |
| `image.pullPolicy`              | Docker image pull policy                                                               | `IfNotPresent`    |
| `image.pullSecrets`             | Specify image pull secrets                                                             | `nil`             |
| `config`                        | A string to use as the `vector.toml` configuration file                                | see [values.yaml] |
| `data.hostPath`                 | Path on the host to mount to `/var/lib/vector` in the container.                       | `/var/lib/vector` |
| `command`                       | Custom command (Docker Entrypoint)                                                     | `[]`              |
| `args`                          | Custom args (Docker Cmd)                                                               | `[]`              |
| `extraVars`                     | A list of additional environment variables                                             | `[]`              |
| `extraVolumes`                  | Add additional volumes                                                                 | `[]`              |
| `extraVolumeMounts`             | Add additional mounts                                                                  | `[]`              |
| `extraSecrets`                  | Add additional secrets                                                                 | `{}`              |
| `extraInitContainers`           | Add additional initContainers                                                          | `[]`              |
| `resources`                     |                                                                                        | `{}`              |
| `priorityClassName`             | priorityClassName                                                                      | `nil`             |
| `nodeSelector`                  |                                                                                        | `{}`              |
| `annotations`                   |                                                                                        | `{}`              |
| `tolerations`                   |                                                                                        | `[]`              |
| `affinity`                      |                                                                                        | `{}`              |
| `rbac.create`                   | Specifies whether RBAC resources should be created                                     | `true`            |
| `serviceAccount.create`         | Specifies whether a ServiceAccount should be created                                   | `true`            |
| `serviceAccount.name`           | the name of the ServiceAccount to use                                                  | `""`              |
| `podSecurityPolicy.enabled`     | Should the PodSecurityPolicy be created. Depends on `rbac.create` being set to `true`. | `false`           |
| `podSecurityPolicy.annotations` | Annotations to be added to the created PodSecurityPolicy:                              | `""`              |
| `privileged`                    | Specifies wheter to run as privileged                                                  | `false`           |

Specify each parameter using the `--set key=value[,key=value]` argument to `helm install`.

Alternatively, a YAML file that specifies the values for the parameters can be provided while installing the chart. For example,

```bash
$ helm install --name vector -f values.yaml stable/vector
```
