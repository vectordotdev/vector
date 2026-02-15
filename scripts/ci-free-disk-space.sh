#!/usr/bin/env bash
# Based on cleanup script from: https://github.com/apache/flink
# Licensed under Apache License 2.0

# Parse command line arguments
SKIP_DOCKER=false
for arg in "$@"; do
  case $arg in
    --no-docker)
      SKIP_DOCKER=true
      shift
      ;;
  esac
done

echo "=============================================================================="
echo "Freeing up disk space on GitHub Actions runner"
echo "=============================================================================="

echo "Disk space before cleanup:"
df -h /

echo "Removing large packages..."
sudo apt-get remove -y '^dotnet-.*' '^llvm-.*' 'php.*' '^mongodb-.*' '^mysql-.*' \
  azure-cli google-cloud-sdk hhvm google-chrome-stable firefox powershell mono-devel libgl1-mesa-dri 2>/dev/null || true
sudo apt-get autoremove -y
sudo apt-get clean

echo "Removing large directories..."
sudo rm -rf /usr/share/dotnet/ \
  /usr/local/graalvm/ \
  /usr/local/.ghcup/ \
  /usr/local/share/powershell \
  /usr/local/share/chromium \
  /usr/local/lib/android \
  /opt/hostedtoolcache/CodeQL \
  /usr/local/lib/android/sdk \
  /usr/share/swift \
  /opt/az

if [[ "$SKIP_DOCKER" == "false" ]]; then
  echo "Cleaning Docker artifacts..."
  docker system prune -af --volumes || true
else
  echo "Skipping Docker cleanup (--no-docker flag provided)"
fi

echo "Cleaning swap..."
sudo swapoff -a || true
sudo rm -f /mnt/swapfile || true

echo "Disk space after cleanup:"
df -h /
