"""
Compare snakemake-lang compilation output against legacy parser.py.

Usage: python tests/compare_parsers.py tests/fixtures/*.smk

Uses `snakemake --print-compilation` for the legacy output and
`snakemake_lang.parse_and_compile()` for the new output.
"""
import subprocess
import sys
import difflib
from pathlib import Path


def compile_legacy(path: Path) -> str:
    """Compile using snakemake --print-compilation."""
    result = subprocess.run(
        ["snakemake", "--snakefile", str(path), "--print-compilation"],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip())
    return result.stdout


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
            legacy_output = compile_legacy(path)
        except Exception as e:
            print(f"OLD_ERR:  {path}: {e}")
            # Not a failure of our parser
            successes += 1
            continue

        legacy_lines = normalize(legacy_output)
        new_lines = normalize(new_output)

        if legacy_lines != new_lines:
            diff = list(
                difflib.unified_diff(
                    legacy_lines,
                    new_lines,
                    fromfile=f"{path} (parser.py)",
                    tofile=f"{path} (snakemake-lang)",
                    lineterm="",
                )
            )
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
