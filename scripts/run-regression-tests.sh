#!/usr/bin/env bash
set -euo pipefail

node scripts/ui-regression.test.mjs
node scripts/document-scroll-restore.test.mjs
node scripts/workbench-scroll-regression.test.mjs
node scripts/scroll-log-tracepoints.test.mjs
node scripts/scroll-log-persistence.test.mjs
node scripts/packaging-regression.test.mjs
node scripts/placeholder-sync.test.mjs
