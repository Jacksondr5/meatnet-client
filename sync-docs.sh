#!/usr/bin/env bash

DOCS_DIR="external-docs"
DOCS_REPO="https://github.com/combustion-inc/combustion-documentation.git"

if [ -d "$DOCS_DIR/.git" ]; then
  echo "Updating docs..."
  git -C "$DOCS_DIR" pull
else
  echo "Cloning docs..."
  git clone "$DOCS_REPO" "$DOCS_DIR"
fi