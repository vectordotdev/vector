# cat Cargo.toml | grep "version" | head -n 1 | cut -f3 -d' ' | tr -d '"'
git describe --abbrev=0 --tags
