#!/usr/bin/env python3
"""Discover functions/methods below a coverage threshold.

Usage:
    python discover_targets.py [--threshold 70] [--branch] [--source-dir src/]

Outputs a JSON list of targets suitable for adversarial test generation:
    [
        {
            "file": "src/pipelines/crude.py",
            "function": "get_production",
            "line_start": 42,
            "line_end": 67,
            "line_coverage_pct": 45.0,
            "missing_lines": [55, 58, 59, 60, 63, 64, 65],
            "missing_branches": [[50, 55], [62, 65]]
        }
    ]

Prerequisites:
    - pytest, pytest-cov, coverage must be installed
    - A working test suite that can be run with pytest
"""

import argparse
import ast
import json
import subprocess
import sys
from pathlib import Path


def run_coverage(source_dir: str, branch: bool) -> Path:
    """Run pytest with coverage and produce a JSON report."""
    cmd = [
        sys.executable,
        "-m",
        "pytest",
        f"--cov={source_dir}",
        "--cov-report=json:.coverage.json",
        "--no-header",
        "-q",
    ]
    if branch:
        cmd.append("--cov-branch")

    print(f"Running: {' '.join(cmd)}", file=sys.stderr)
    result = subprocess.run(cmd, capture_output=True, text=True)

    if result.returncode not in (0, 1):  # 1 = some tests failed, still valid coverage
        print(f"pytest failed:\n{result.stderr}", file=sys.stderr)
        sys.exit(1)

    coverage_path = Path(".coverage.json")
    if not coverage_path.exists():
        print("Coverage JSON report not generated.", file=sys.stderr)
        sys.exit(1)

    return coverage_path


def extract_functions(filepath: str) -> list[dict]:
    """Parse a Python file and extract function/method definitions with line ranges."""
    source = Path(filepath).read_text()
    tree = ast.parse(source, filename=filepath)

    # Build parent map in single pass (O(n) instead of O(n²))
    parent_map = {}
    for node in ast.walk(tree):
        for child in ast.iter_child_nodes(node):
            parent_map[child] = node

    functions = []
    for node in ast.walk(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            # Skip private/dunder unless they're __init__ with logic
            if node.name.startswith("_") and node.name != "__init__":
                continue

            end_line = node.end_lineno or node.lineno
            parent = parent_map.get(node)
            parent_class = parent.name if isinstance(parent, ast.ClassDef) else None

            qualified_name = f"{parent_class}.{node.name}" if parent_class else node.name
            functions.append(
                {
                    "function": qualified_name,
                    "line_start": node.lineno,
                    "line_end": end_line,
                }
            )

    return functions


def compute_function_coverage(
    file_path: str,
    file_coverage: dict,
    functions: list[dict],
    threshold: float,
) -> list[dict]:
    """Compute per-function coverage and return those below threshold."""
    executed = set(file_coverage.get("executed_lines", []))
    missing = set(file_coverage.get("missing_lines", []))
    missing_branches = file_coverage.get("missing_branches", [])

    targets = []
    for func in functions:
        func_lines = set(range(func["line_start"], func["line_end"] + 1))
        func_relevant = func_lines & (executed | missing)

        if not func_relevant:
            continue

        func_executed = func_relevant & executed
        func_missing = sorted(func_relevant & missing)

        line_pct = (len(func_executed) / len(func_relevant)) * 100 if func_relevant else 100

        # Branch coverage for lines in this function
        func_missing_branches = [b for b in missing_branches if any(line in func_lines for line in b)]

        if line_pct < threshold:
            targets.append(
                {
                    "file": file_path,
                    "function": func["function"],
                    "line_start": func["line_start"],
                    "line_end": func["line_end"],
                    "line_coverage_pct": round(line_pct, 1),
                    "missing_lines": func_missing,
                    "missing_branches": func_missing_branches,
                }
            )

    return targets


def main() -> None:
    parser = argparse.ArgumentParser(description="Find under-covered functions for adversarial testing.")
    parser.add_argument(
        "--threshold",
        type=float,
        default=70.0,
        help="Coverage percentage below which functions are targeted (default: 70)",
    )
    parser.add_argument(
        "--branch",
        action="store_true",
        help="Enable branch coverage analysis",
    )
    parser.add_argument(
        "--source-dir",
        default="src/",
        help="Source directory to measure coverage for (default: src/)",
    )
    parser.add_argument(
        "--coverage-json",
        default=None,
        help="Path to existing coverage JSON (skip test run if provided)",
    )
    args = parser.parse_args()

    # Get coverage data
    if args.coverage_json:
        coverage_path = Path(args.coverage_json)
    else:
        coverage_path = run_coverage(args.source_dir, args.branch)

    with open(coverage_path) as f:
        coverage_data = json.load(f)

    # Process each file
    all_targets = []
    for file_path, file_cov in coverage_data.get("files", {}).items():
        if not file_path.endswith(".py"):
            continue
        if not Path(file_path).exists():
            continue

        functions = extract_functions(file_path)
        targets = compute_function_coverage(file_path, file_cov, functions, args.threshold)
        all_targets.extend(targets)

    # Sort by coverage (lowest first = most interesting)
    all_targets.sort(key=lambda t: t["line_coverage_pct"])

    print(json.dumps(all_targets, indent=2))
    print(f"\n# {len(all_targets)} targets below {args.threshold}% coverage", file=sys.stderr)


if __name__ == "__main__":
    main()
