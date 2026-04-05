# Output Format

Two separate artifacts: test files (executable) and summary report (analysis). Classification lives in the report, not in the tests.

## Test File

Test code only. No classification or analysis in docstrings.

```python
def test_<target>_<scenario>_<expected_behavior>():
    """<one-line description of what the test verifies>."""
    # Arrange / Act / Assert
```

Example:

```python
def test_get_production_raises_for_unknown_refinery():
    """get_production should raise KeyError for unknown refinery IDs."""
    with pytest.raises(KeyError, match="NONEXISTENT"):
        get_production(refinery_id="NONEXISTENT")
```

## Summary Report

Print to stdout after writing test files. Classification, evidence, and impact live here.

```
## White-Box Red Testing Report
Scope: <description>
Targets analyzed: N
Findings: M (X confirmed-bug, Y likely-bug, Z specification-gap)

### confirmed-bug
- file:function — one-line description
  Evidence: <what contract says>
  Impact: <who gets hurt and how>

### likely-bug
- file:function — one-line description
  Evidence: <implicit contract>
  Impact: <what breaks>

### specification-gap
- file:function — one-line description
  Ambiguity: <what's unclear>

### Confidence (tested and passed)
- parse_date(valid_iso) × boundary_input → no finding
- process_batch(empty_list) × missing_guard → no finding
- ...
```
