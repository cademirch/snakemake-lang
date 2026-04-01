"""
Compare snakemake-lang compilation output against legacy parser.py.

Usage: python tests/compare_parsers.py tests/fixtures/*.smk

Requires: snakemake and snakemake-lang installed in the same environment.
"""
import io
import sys
import difflib
from pathlib import Path
from unittest.mock import MagicMock


def compile_legacy(source: str, path: str) -> str:
    """Compile using snakemake's built-in parser with a mock workflow."""
    from snakemake.parser import parse as snakemake_parse

    class FakeSourceFile:
        def get_path_or_uri(self, secret_free=False):
            return path

    workflow = MagicMock()
    workflow.sourcecache.open.return_value = io.StringIO(source)

    linemap: dict[int, int] = {}
    code, _ = snakemake_parse(FakeSourceFile(), workflow, linemap)
    return code


def compile_new(source: str, path: str) -> str:
    """Compile using snakemake-lang."""
    from snakemake_lang import parse_and_compile
    code, _ = parse_and_compile(source, path)
    return code


def normalize(code: str) -> list[str]:
    """Normalize whitespace for comparison."""
    return [line.rstrip() for line in code.splitlines() if line.strip()]


def main():
    files = [Path(f) for f in sys.argv[1:]]
    if not files:
        print("Usage: python tests/compare_parsers.py <files...>")
        sys.exit(1)

    failures = []
    successes = 0

    for path in files:
        source = path.read_text()
        try:
            new_output = compile_new(source, str(path))
        except Exception as e:
            print(f"NEW_ERR:  {path}: {e}")
            failures.append((path, f"snakemake-lang error: {e}"))
            continue

        try:
            legacy_output = compile_legacy(source, str(path))
        except Exception as e:
            # Legacy parser may not handle our breaking changes
            print(f"OLD_ERR:  {path}: {e}")
            successes += 1
            continue

        legacy_lines = normalize(legacy_output)
        new_lines = normalize(new_output)

        if legacy_lines != new_lines:
            diff = list(difflib.unified_diff(
                legacy_lines, new_lines,
                fromfile=f"{path} (parser.py)",
                tofile=f"{path} (snakemake-lang)",
                lineterm=""
            ))
            if diff:
                failures.append((path, "\n".join(diff)))
                print(f"DIFF: {path}")
            else:
                successes += 1
                print(f"OK:   {path}")
        else:
            successes += 1
            print(f"OK:   {path}")

    print(f"\n{successes} OK, {len(failures)} differ")

    if failures:
        for path, detail in failures:
            print(f"\n--- {path} ---")
            print(detail)
        sys.exit(1)


if __name__ == "__main__":
    main()
