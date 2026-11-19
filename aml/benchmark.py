"""Reproducible source-size benchmarks for AML's token-efficiency goal."""

from __future__ import annotations

from importlib import resources
import json
import math
import re
from typing import Callable


CASE_NAMES = ("validated_greeting", "bounded_score", "safe_percentage")
LANGUAGES = {"python": "py", "javascript": "js", "ruby": "rb", "go": "go"}


def _counter() -> tuple[str, Callable[[str], int]]:
    """Prefer a real model tokenizer, with an explicit approximation fallback."""
    try:
        import tiktoken  # type: ignore[import-not-found]

        encoding = tiktoken.get_encoding("cl100k_base")
        return "cl100k_base", lambda text: len(encoding.encode(text))
    except (ImportError, ValueError):
        # Four characters per token is a coarse English/code estimate. Rounding
        # each lexical segment avoids claiming sub-token precision.
        def estimate(text: str) -> int:
            segments = re.findall(r"[A-Za-z_][A-Za-z0-9_]*|\d+(?:\.\d+)?|\S", text)
            return sum(max(1, math.ceil(len(segment) / 4)) for segment in segments)

        return "lexical_estimate", estimate


def _case_source(case: str, language: str) -> str:
    package = resources.files("aml.benchmark_cases")
    return package.joinpath(f"{case}.{language}").read_text(encoding="utf-8")


def benchmark_results(language: str = "all") -> dict[str, object]:
    method, count = _counter()
    if language != "all" and language not in LANGUAGES:
        raise ValueError(f"unknown benchmark language: {language}")
    selected = LANGUAGES if language == "all" else {language: LANGUAGES[language]}
    comparisons: dict[str, object] = {}

    for language_name, extension in selected.items():
        cases: list[dict[str, object]] = []
        aml_total = 0
        comparison_total = 0
        for name in CASE_NAMES:
            aml_source = _case_source(name, "aml")
            comparison_source = _case_source(name, extension)
            aml_tokens = count(aml_source)
            comparison_tokens = count(comparison_source)
            aml_total += aml_tokens
            comparison_total += comparison_tokens
            saving = (
                0.0
                if not comparison_tokens
                else (comparison_tokens - aml_tokens) / comparison_tokens
            )
            cases.append(
                {
                    "case": name,
                    "aml_tokens": aml_tokens,
                    "comparison_tokens": comparison_tokens,
                    "source_reduction_percent": round(saving * 100, 1),
                }
            )
        total_saving = (
            0.0
            if not comparison_total
            else (comparison_total - aml_total) / comparison_total
        )
        comparisons[language_name] = {
            "cases": cases,
            "totals": {
                "aml_tokens": aml_total,
                "comparison_tokens": comparison_total,
                "tokens_saved": comparison_total - aml_total,
                "source_reduction_percent": round(total_saving * 100, 1),
            },
        }

    return {
        "tokenizer": method,
        "scope": "authored source only",
        "comparisons": comparisons,
    }


def run_benchmarks(as_json: bool = False, language: str = "all") -> str:
    results = benchmark_results(language=language)
    if as_json:
        return json.dumps(results, indent=2, sort_keys=True)

    rows = [
        "AML source benchmark",
        f"Tokenizer: {results['tokenizer']}",
        f"Scope: {results['scope']}",
    ]
    for language_name, comparison in results["comparisons"].items():  # type: ignore[union-attr]
        label = language_name.title()
        rows.extend(
            [
                "",
                f"AML vs {label}",
                f"{'case':<22} {'AML':>7} {label:>10} {'reduction':>11}",
                f"{'-' * 22} {'-' * 7} {'-' * 10} {'-' * 11}",
            ]
        )
        for case in comparison["cases"]:
            rows.append(
                f"{case['case']:<22} {case['aml_tokens']:>7} "
                f"{case['comparison_tokens']:>10} "
                f"{case['source_reduction_percent']:>10.1f}%"
            )
        totals = comparison["totals"]
        rows.append(
            f"{'TOTAL':<22} {totals['aml_tokens']:>7} "
            f"{totals['comparison_tokens']:>10} "
            f"{totals['source_reduction_percent']:>10.1f}%"
        )
    rows.extend(
        [
            "",
            "This measures authored source, not prompts, reasoning, repairs, or runtime.",
        ]
    )
    return "\n".join(rows)

