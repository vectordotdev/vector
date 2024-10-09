#!/bin/bash

# Usage: ./retry.sh -r <max_retries> -d <delay_in_seconds> -c "<command>"
# Example: ./retry.sh -r 5 -d 30 -c "choco install protoc"

max_retries=3
delay=10
command=""

# Parse options
while getopts "r:d:c:" opt; do
  case $opt in
    r)
      max_retries=$OPTARG
      ;;
    d)
      delay=$OPTARG
      ;;
    c)
      command=$OPTARG
      ;;
    \?)
      echo "Invalid option: -$OPTARG" >&2
      exit 1
      ;;
    :)
      echo "Option -$OPTARG requires an argument." >&2
      exit 1
      ;;
  esac
done

# Check if command is provided
if [[ -z "$command" ]]; then
  echo "Usage: $0 -r <max_retries> -d <delay_in_seconds> -c \"<command>\""
  exit 1
fi

attempt=1

while [[ $attempt -le $max_retries ]]; do
  echo "Attempt $attempt to run: $command"

  if eval "$command"; then
    echo "Command succeeded on attempt $attempt."
    exit 0
  else
    echo "Attempt $attempt failed. Retrying in $delay seconds..."
    ((attempt++))
    sleep $delay
    delay=$((delay * 2))  # Exponential backoff (increase delay each time)
  fi

  if [[ $attempt -gt $max_retries ]]; then
    echo "Command failed after $max_retries attempts."
    exit 1
  fi
done
