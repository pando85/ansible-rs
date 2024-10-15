#!/bin/bash
set -e

# bump version
vim Cargo.toml
make update-version

# update lock file
cargo update -p rash_core -p rash_derive

make update-changelog

git add .
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
git commit -m "release: Version $VERSION"

echo "After merging the PR, tag and release are automatically done"