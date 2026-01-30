#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
proto_root="${root}/schemas"
out="${proto_root}/googleads.desc"

mapfile -t protos < <(fd --type f -e proto . "${proto_root}")
if [[ ${#protos[@]} -eq 0 ]]; then
  echo "no protos found under ${proto_root}" >&2
  exit 1
fi

protoc -I "${proto_root}" --descriptor_set_out="${out}" --include_imports "${protos[@]}"

echo "wrote: ${out}"
