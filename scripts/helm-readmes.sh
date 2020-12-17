#!/usr/bin/env bash
set -euo pipefail

# helm-readmes.sh
#
# SUMMARY
#
#   Update Helm chart Readme' with templated data from Cue
#
#   Most of the data we'd want in the readme is already in our
#   Cue documentation so instead of writing a manual Readme for
#   each you can use this script to update the existing readmes
#   from the templated data. We don't update inline, we delete
#   the existing README.md files, if they exist and recreate.
#   This script takes a while to run and only supports the
#   Agent mode currently until future decisions on Aggregator
#   accessibility are determined
#
#   DEPENDENCIES: Linux, Cue, tr, sed, bash, awk, jq
AGENT="./distribution/helm/vector-agent/README.md"
AGGREGATOR="./distribution/helm/vector-aggregator/README.md"
UNIFIED="./distribution/helm/vector/README.md"
ROOT_DATA="./out.tmp"

cd "$(dirname "${BASH_SOURCE[0]}")/.."

verify_deps() {
  local DEPENDENCIES=(cue tr sed bash awk jq)

  for i in "${DEPENDENCIES[@]}"; do
    if ! command -v "${i}" &> /dev/null; then
      echo "*********************************"
      echo "COMMAND ${i} could not be found. Please install and restart this script"
      echo "*********************************"
      echo
      exit
    fi
  done
}

verify_readmes() {
  local READMES=("${AGENT}" "${AGGREGATOR}" "${UNIFIED}")
  for i in "${READMES[@]}"; do
    if [[ -f "${i}" ]]; then
      echo "*********************************"
      echo "Previous version exists at ${i}"
      echo " Please rerun with 'update' subcommand"
      echo "*********************************"
      echo
      exit 1
    fi
  done
}

remove_old() {
  local READMES=("${AGENT}" "${AGGREGATOR}" "${UNIFIED}")
  for i in "${READMES[@]}"; do
    if [[ -f "${i}" ]]; then
      echo "*********************************"
      echo "Removing old version of ${i}"
      echo "*********************************"
      echo
      rm "${i}"
    fi
  done
}

clean_tmpfiles() {
  if [[ -f ${ROOT_DATA} ]]; then
    echo "*********************************"
    echo "Existing tmpfile detected. Removing ${ROOT_DATA}"
    echo "*********************************"
    echo
    rm ${ROOT_DATA}
  fi
}

generate_readme() {
  clean_tmpfiles
  echo "*********************************"
  echo "Generating tmpfile from cue data"
  echo "*********************************"
  echo

  ./scripts/cue.sh export ./docs/**/*.cue | jq -r '.installation.operating_systems.ubuntu.interfaces[] | select(.title == "Helm 3")'>${ROOT_DATA}

  local DESCRIPTION=$(cat "${ROOT_DATA}" | jq -r .roles.${1}.description | tr '\n' ' ')
  local ADD_REPO=$(cat "${ROOT_DATA}" | jq -r .roles.${1}.commands.add_repo)
  local CHECK_OPTS=$(cat "${ROOT_DATA}" | jq -r .roles.${1}.commands.helm_values_show)
  local VECTOR_CONFIG=$(cat "${ROOT_DATA}" | jq -r .roles.${1}.commands.configure | awk '{gsub(/\\n/,"\n")}1')
  local VECTOR_INSTALL=$(cat "${ROOT_DATA}" | jq -r .roles.${1}.commands.install | awk '{gsub(/\\n/,"\n")}1')

  if [ "${1}" == "agent" ]; then
     local README="${AGENT}"
  elif [ "${1}" == "aggregator" ]; then
    local README="${AGGREGATOR}"
  elif [ "${1}" == "unified" ]; then
    local README="${UNIFIED}"
  fi

  echo "*********************************"
  echo "Writing Agent README.md"
  echo "*********************************"
  echo
  sh -c "cat > ${README}" <<EOT
# [Vector](https://vector.dev) Helm Chart

This is an opinionated Helm Chart for running [Vector](https://vector.dev) in Kubernetes.

Our charts use Helm's dependency system, however we only use local dependencies.
Head over to the repo for [more information on development and contribution](https://github.com/timberio/vector/tree/master/distribution/helm).

${DESCRIPTION}

To get started add the Helm chart repo
\`\`\`
${ADD_REPO}
\`\`\`

Check the available Helm chart configuration options
\`\`\`
${CHECK_OPTS}
\`\`\`

Set up a Vector config  that leverages our \`kubernetes_logs\` data source
\`\`\`
${VECTOR_CONFIG}
\`\`\`

To install the chart
\`\`\`
${VECTOR_INSTALL}
\`\`\`

To update
\`\`\`
${VECTOR_INSTALL}
\`\`\`
EOT
  clean_tmpfiles
}

update() {
  verify_deps
  remove_old
  generate
}

generate() {
  verify_deps
  verify_readmes
  generate_readme agent
}


usage() {
  cat >&2 <<-EOF
Usage: $0 MODE

Modes:
  generate   - generate Helm chart readmes from templated docs.
  update     - destructive operation, removes any existing readmes and generates
             fresh ones.
EOF
  exit 1
}

MODE="${1:-}"
case "$MODE" in
update | generate )
  "$MODE"
  ;;
*)
  usage
  ;;
esac
