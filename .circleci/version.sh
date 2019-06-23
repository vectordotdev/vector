# cat Cargo.toml | grep "version" | head -n 1 | cut -f3 -d' ' | tr -d '"'
# git describe --abbrev=0 --tags

version=''

if [ -n "$CIRCLE_TAG" ]
then
  version=git describe --abbrev=0 --tags
elif [ -n "$CIRCLE_BRANCH" ]
then
  version=$(git describe --tags)
else
  echo "error: neither TAG nor BRANCH was set"
  exit 1
fi

# Remove the leading v since this is not a valid version
echo $version | sed 's/^v//g'