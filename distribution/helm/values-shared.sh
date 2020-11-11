#!/bin/bash
shared-globals() {
  cat <<'EOF'
# Global values can be consumed by both Parent and Child Helm Charts
# Each chart yml file can leverage each of these values where applicable
#global:
#  vector:
#    image:
#      repository: <docker repo>
#      #  Overrides the image tag, the default is `{image.version}-{image.base}`
#      tag: <tag>
#      # Overrides the image version, the default is the Chart appVersion.
#      version: <version>
#      # Sets the image base OS
#      base: <base>
#  # Sets common environment variables
#  commonEnvKV:
#    - name: "CHIQUITA"
#      value: "banana"
EOF
}
