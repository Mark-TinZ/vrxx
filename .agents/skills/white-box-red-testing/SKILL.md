---
name: white-box-red-testing
description: >
  Find bugs by writing tests that should pass but don't. Invoke manually on user-chosen scope
  (commits, files, or coverage threshold). Outputs red tests with structured rationale.
  Use when user asks to "stress-test", "find bugs in", "attack", or "break" code.
---

# White-Box Red Testing

## Hard Boundaries

- NEVER modify source code. Analyze and classify only.
- NEVER weaken existing tests.
- Unexported/private members: test only if explicitly scoped.
- Output to user-specified directory or language-appropriate default.

## When to Use

- You have specific targets: changed files, a commit range, or low-coverage functions
- You need classified findings (confirmed-bug / likely-bug / specification-gap)
- Contracts exist (docstrings, types, tests) and you want to verify code matches them
- After a commit or PR — "did these changes introduce bugs?"

If you don't know where to look or want to explore broadly, use the black-box-red-testing skill instead.

## Scope Modes

User chooses scope. Never run unsolicited.

| Mode | Trigger | Targets |
|------|---------|---------|
| Commits | `--commits HEAD~3..HEAD` | Changed/added functions in diff |
| Files | `--files <path>` or `--module <dir>` | All public callables in specified paths |
| Coverage | `--coverage-below 70 [--branch]` | Functions below threshold via `scripts/discover_targets.py` |

**Note on coverage targeting:** Low coverage indicates under-tested code, not necessarily buggy code. Use as a targeting heuristic to prioritize where to look, not as a bug predictor.

## Workflow

```
1. IDENTIFY TARGETS
   - commits: git diff → parse changed functions
   - files: AST parse → list public callables
   - coverage: run scripts/discover_targets.py

2. GATHER CONTRACT EVIDENCE per target
   - docstrings, type annotations, existing passing tests
   - function/param names, assertions, call sites, commit messages
   - No evidence? → findings become "specification-gap"

3. FORM HYPOTHESES per target as structured one-liners:
   [code_path] × [defect_class] → [observable_symptom]

   Defect classes to consider:
   - boundary inputs (empty, zero, None, unicode, tz-naive)
   - state/mutation (shared state, call sequences, input mutation)
   - implicit contracts (name promises vs actual behavior)
   - error paths (timeouts, missing data, malformed input)

4. GENERATE ADVERSARIAL TESTS — one test per hypothesis

5. SELF-VALIDATE (mandatory)
   - Run all generated tests
   - Red → candidate finding
   - Green → record hypothesis in confidence section of report
   - Broken → fix or discard

6. CLASSIFY per references/finding-classification.md
   - confirmed-bug | likely-bug | specification-gap

7. APPLY DISTINCTNESS FILTER
   Each finding must differ from others in at least one of:
   - The code path exercised
   - The category of defect found
   - The component boundary tested
   Shallow variations of the same finding → consolidate into one.

8. OUTPUT
   - Test files to output directory (test code only, no classification in tests)
   - Summary report to stdout (classification, evidence, impact per finding)
   - Format: see references/output-format.md
```

## Constraints

- Coverage scope mode requires Python + `pytest-cov`. Commits and Files modes are language-agnostic.
- Coverage scope uses `scripts/discover_targets.py` — run with `--help` for options
- If >50 targets, ask user to narrow scope

## Stop Conditions

- Max 5 generation attempts per target — if 3 attempts yield nothing, shift defect category (boundary → state → contract → error path) rather than retrying the same angle
- >50% of targets yield no findings → report as confidence signal for tested areas, continue with remaining targets
- >15 findings before all targets analyzed → pause for triage
