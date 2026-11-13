#!/bin/bash
# The emulator serves only HTTPS, with a certificate made at startup that names only loopback
# addresses. The tests reach it by its compose service name, so bridge plain HTTP on port 4566
# to the emulator's HTTPS port on loopback.
#
# The emulator makes new credentials at each start and writes them to
# ~/.aws/emulator/9297/emulator_credentials. That directory is a volume that the test runner
# also mounts, so the tests sign their requests with these credentials.
set -euo pipefail

EMULATOR=https://127.0.0.1:9297
CREDENTIALS="$HOME/.aws/emulator/9297/emulator_credentials"

# Don't let the tests read the credentials of an earlier run before this run replaces them.
rm -f "$CREDENTIALS"

aws-emulator --port 9297 --telemetry off &

for _ in $(seq 60); do
  curl -ksf "$EMULATOR/__admin/health" >/dev/null && break
  sleep 1
done

credential() {
  sed -n "s/^$1 = //p" "$CREDENTIALS"
}

# Firehose refuses delivery streams whose role it can't assume, and drops deliveries that the
# role isn't allowed to write. The SDK crates Vector depends on don't include IAM, so create the
# role that the Firehose tests use here.
iam() {
  curl -ksSf --aws-sigv4 "aws:amz:us-east-1:iam" \
    --user "$(credential aws_access_key_id):$(credential aws_secret_access_key)" "$EMULATOR/" \
    --data-urlencode Version=2010-05-08 "$@" >/dev/null
}
iam --data-urlencode Action=CreateRole --data-urlencode RoleName=firehose-delivery \
  --data-urlencode 'AssumeRolePolicyDocument={"Version":"2012-10-17","Statement":[{"Effect":"Allow","Principal":{"Service":"firehose.amazonaws.com"},"Action":"sts:AssumeRole"}]}'
iam --data-urlencode Action=PutRolePolicy --data-urlencode RoleName=firehose-delivery \
  --data-urlencode PolicyName=s3 \
  --data-urlencode 'PolicyDocument={"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"s3:*","Resource":"*"}]}'

socat TCP-LISTEN:4566,fork,reuseaddr "OPENSSL:127.0.0.1:9297,verify=0,snihost=localhost" &

# Exit as soon as either process exits, so a crash fails the test run.
wait -n
