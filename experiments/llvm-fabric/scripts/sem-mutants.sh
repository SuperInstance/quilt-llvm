#!/usr/bin/env bash
# R1 lane 3 (tier S2) — code-level semantic sabotage battery.
#
# Runs 20 one-line, type-correct, semantically wrong source mutations
# through two judges (the suite; the suite + property-oracle fixture),
# restoring the tree after each. Published table:
# docs/phase/SEM-MUTANTS.md; raw results: scripts/sem-mutants-results.tsv.
set -euo pipefail
cd "$(dirname "$0")/.."
exec python3 scripts/sem_mutants_driver.py "$@"
