# Finding Classification

Three tiers. Every finding must be classified before inclusion in the report.

## confirmed-bug

**Criteria**: ALL of the following hold:
- Explicit contract evidence exists: docstring makes a testable behavioral claim, OR type
  annotation constrains the return value, OR existing test establishes behavioral precedent
- The observed behavior directly contradicts the contract
- At least one call site exists in the codebase (grep-verifiable)

**Example**: Docstring says "raises ValueError for negative input." Function silently
returns None for negative input. Existing tests only cover positive inputs.

**Action for consumer**: Fix the code or update the contract. Either way, something is wrong.

## likely-bug

**Criteria**: ALL of the following hold:
- Contract evidence is implicit but verifiable (function name implies behavior, parameter
  names suggest constraints, call sites assume specific return shape)
- The observed behavior contradicts what the name/usage pattern promises
- No explicit documentation says "this is intentional"

**Example**: Function `calculate_total_volume` returns a negative number when one input
stream has zero flow. No docstring, but the name and all call sites assume non-negative
return values.

**Action for consumer**: Investigate. Probably a bug, but may be intentional undocumented
behavior. Either way, the contract needs clarification.

## specification-gap

**Criteria**: ANY of the following hold:
- No contract evidence exists (no docstring, no types, no tests, generic names)
- Multiple reasonable interpretations of correct behavior exist
- The test exposes an ambiguity, not a clear violation

**Example**: Function `get_data(date)` accepts both `str` and `datetime` but behaves
differently for each — returns daily data for datetime, monthly for string. No
documentation explains this. Both could be "correct."

**Action for consumer**: Write a docstring or contract. The test has exposed a place where
the next maintainer will guess wrong. The adversarial test may become a regression test
once the intended behavior is decided.

## Classification Decision Tree

```
Has explicit contract evidence (docstring claim, type constraint, test precedent)?
├── YES → Does observed behavior contradict it?
│         ├── YES → Has call site in codebase?
│         │         ├── YES → confirmed-bug
│         │         └── NO  → discard (dead code)
│         └── NO  → discard (code is correct)
└── NO  → Is there implicit evidence (names imply behavior, call sites assume shape)?
          ├── YES → Does behavior contradict what name/usage promises?
          │         ├── YES → likely-bug
          │         └── NO  → discard
          └── NO  → Is behavior surprising or inconsistent?
                    ├── YES → specification-gap
                    └── NO  → discard
```

## Severity Tags (Optional)

Within each tier, findings may be tagged:

- **silent-corruption**: Wrong data propagates without error. Highest severity.
- **caller-trap**: Next person to call this function will likely misuse it.
- **maintainer-trap**: Next person to edit this function will likely break it.
- **edge-only**: Only triggered by unusual but possible inputs.
