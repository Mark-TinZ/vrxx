---
name: black-box-red-testing
description: Black-Box Red Testing — red tests that expose real bugs
---

Write tests that catch, not tests that pass.

# Objective

Produce tests that fail against real bugs. A good test is one that exposes a bug the codebase actually has (row 2 of the Test Truth Table in the Testing skill).

The agent needs bug hypotheses, not buggy code. The skill never modifies the codebase — it only writes tests.

# When to Use

- Exploring an area for unknown bugs — you don't know where to look
- Specs are weak or absent — spec gap exploitation is this skill's strength
- During code review — single round (default) as a lightweight probe
- You want a confidence signal — "is this module solid?"

If you have specific targets (changed files, low-coverage functions) or need classified findings, use the white-box-red-testing skill instead.

# Input

**Target scope:** A module, package, or file set to test adversarially.

If the target scope is large (many packages, broad module), narrow before starting. Adversarial testing is deep, not wide — pick the riskiest or most complex area first.

Before starting, assess the target:
- Read the code under test
- Read available specs (if any)
- Identify which situation applies:
  1. **Clear specs** — spec is the oracle. Hypothesize bugs in the code.
  2. **Spec gaps** — ambiguity is the attack surface. See Spec Gap Exploitation below.

# Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `rounds` | 1 | Number of rounds. Each round produces 10 tests. |

# Round Protocol

**10 tests per round.** Each round is a structured exploration budget. When running multiple rounds, each is a chance to pivot — if a direction isn't productive, refocus on another part of the code or another kind of bug.

## Per Round

### 1. Hypothesis Formation

Before hypothesizing, read the target's contracts:
- Docstrings, type annotations, function/parameter names
- Existing tests (what's covered, what's not)
- Call sites (how the code is actually used)
- Commit messages on recent changes (what was the intent?)

These are inputs to sharper hypotheses, not classification evidence.

Form 10 distinct bug hypotheses for this round. A senior engineer knows the typical bugs — think:
- Boundary conditions, off-by-one, empty/nil inputs
- State corruption, ordering dependencies
- Error handling gaps, missing validation
- Concurrency, race conditions
- Type coercion, encoding issues
- Contract violations between components
- Implicit assumptions in the code

Each hypothesis must be distinct (see Anti-Gaming below).

**Hypothesis format (required):**

Each hypothesis must be stated as a structured one-liner before writing the test:
```
[code_path] × [defect_class] → [observable_symptom]
```

Example output:
```
Hypotheses:
1. parse_date(empty_string) × missing_guard → uncaught ValueError
2. process_batch(>1000_items) × unbounded_loop → timeout
3. auth_token(expired) × stale_cache → false_positive_auth
4. save_record(unicode_name) × encoding_assumption → corrupted_output
5. concurrent_update(same_id) × race_condition → lost_write
...
```

This format forces precision. Vague hypotheses ("some edge case × something → failure") are immediately visible and must be sharpened before proceeding.

### 2. Test Writing

Write one test per hypothesis. Each test should:
- Target a specific, articulable bug hypothesis
- Be deterministic and self-contained
- Follow the codebase's existing test conventions
- Be a valid test — it must compile and run

### 3. Execution

Run all 10 tests against the existing codebase.

### 4. Triage

| Result | Action |
|--------|--------|
| Red | **Keep** — the test exposed something. |
| Green | **Discard** — the code handled this correctly. |
| Error (won't compile, bad setup) | **Fix or discard** — broken tests aren't adversarial, they're wrong. |

### 5. Pivot Decision

After triage, assess:
- What kinds of bugs did this round find (or not find)?
- Is this angle exhausted, or is there more to explore?
- What area of the code or what class of bugs hasn't been tried?

Pivot to a new angle for the next round, or continue deepening a productive one.

## Multi-Round Guidance (rounds > 1)

**Pivot between rounds.** If a round was unproductive, change angle — different area of the code, different class of bugs. Don't repeat the same approach.

**Triage throttle.** If a round produces 5+ red tests, pause for the coder to triage before spending the next round. A wall of red tests is noise, not signal.

## All-Green Rounds

An all-green round from high-quality hypotheses is a **confidence signal**, not waste. It means the code handles those cases correctly. This information is valuable — report it as such.

# Spec Gap Exploitation

When specs are insufficient or absent, ambiguity becomes the attack surface.

A "compliant" red test that exploits a spec gap is a **legitimate find**. The agent is gaming for good — the adversarial move becomes a discovery tool that surfaces gaps the spec should have covered.

The skill does not classify whether a red test points to a code bug or a spec gap. **The skill finds; the coder diagnoses.** This separation mirrors TDD and the programmer/QA role distinction. A red test is a red test.

# Anti-Gaming

The contract's general anti-gaming clause applies. Additionally, within this skill:

**Within a round, tests must represent distinct bug hypotheses.** Ten variations of "nil pointer on empty input" is not ten hypotheses — it's one hypothesis tested ten ways. Vary the *kind* of bug, not just the input to the same bug.

Distinct means different in at least one of:
- The code path exercised
- The category of defect hypothesized
- The component boundary tested

The structured hypothesis format makes gaming visible — if multiple lines share the same `[code_path] × [defect_class]` pattern with only input variations, they must be consolidated into one hypothesis.

# Output

**Surviving red tests only.** Green tests and broken tests are discarded.

Present each surviving red test with:
- The test code
- The failure output
- The target (what code path it exercises)

No hypothesis or classification is attached — the coder owns diagnosis.

If no red tests survive, report the confidence signal: state which areas were tested and what classes of bugs were hypothesized. The structured hypotheses from the round serve as documentation of verified absence.

# Convergence

The skill completes after the configured number of rounds. Early termination is acceptable if:
- A round produced several red tests and the coder wants to address them before continuing
- The target scope is small enough that further rounds would produce redundant hypotheses

# Integration with Testing Skill

This skill operates within the Testing skill's constraints:
- **Test Modification rules apply** — never weaken existing tests
- **Test Truth Table is the reference** — this skill targets row 2 (Buggy code, Red test = Good)
- **Assertion Strength** — adversarial tests should have strong assertions. A weak assertion that passes for broken implementations defeats the purpose.
- **Mocking Discipline** — mock external boundaries, not internal logic. Adversarial tests must exercise real code paths.

Tests written by this skill that survive (stay red) become inputs to the normal development workflow. The coder decides whether to fix the bug, update the spec, or accept the behavior.
