#!/usr/bin/env sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

RELEASE_NAME=${RELEASE_NAME:-lyre}
NAMESPACE=${NAMESPACE:-lyre}

helm upgrade --install "$RELEASE_NAME" "$SCRIPT_DIR" \
  --namespace "$NAMESPACE" \
  --create-namespace \
  "$@"
