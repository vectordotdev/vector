#!/bin/bash
set -euo pipefail

# sync-install.sh
#
# SUMMARY
#
#   Syncs the install.sh script to S3 where it is served for
#   https://sh.vector.com.
#

aws s3 cp distribution/install.sh s3://sh.vector.dev --sse --acl public-read
