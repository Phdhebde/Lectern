#!/usr/bin/env bash
# Fails if a brand name appears in the platform code.
#
# The platform is published without any brand: names, colours and texts come from the
# instance configuration. The deny-list is NOT stored in the repository (it would itself
# leak the names); pass it through the BRAND_DENYLIST environment variable, as a
# comma-separated list, e.g. in CI from a repository variable.
set -euo pipefail

if [[ -z "${BRAND_DENYLIST:-}" ]]; then
  echo "BRAND_DENYLIST is empty: nothing to check (set it in CI)." >&2
  exit 0
fi

pattern=$(echo "$BRAND_DENYLIST" | tr ',' '\n' | sed '/^\s*$/d; s/^\s*//; s/\s*$//' | paste -sd '|')
cd "$(dirname "$0")/.."
if git grep -n -i -I -E "$pattern" -- . ':!LICENSE' ':!*.lock' ':!web/package-lock.json'; then
  echo "error: brand names found in the platform code (see above)." >&2
  exit 1
fi
echo "brand check passed"
