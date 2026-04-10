# /// script
# requires-python = ">=3.12"
# dependencies = ["yamllint", "yamlfix"]
# ///
#
# Requires `uv` installed locally for dependencies above to be installed.
# Run with `uv run scripts/md-yaml.py lint <files>`
#
# See https://packaging.python.org/en/latest/specifications/inline-script-metadata/#example
#
# Quick script to check, feel free to convert to rust/vdev tool.
#
"""Lint and fix YAML code blocks inside markdown files."""

import argparse
import re
import sys
from collections.abc import Iterator
from pathlib import Path

from yamllint import linter
from yamllint.config import YamlLintConfig
from yamlfix import fix_code
from yamlfix.model import YamlfixConfig, YamlNodeStyle

# ---------------------------------------------------------------------------
# Regex constants
# ---------------------------------------------------------------------------

FENCE_OPEN = re.compile(r"^```ya?ml\b", re.IGNORECASE)
FENCE_CLOSE = re.compile(r"^```\s*$")
DIFF_LINE = re.compile(r"^[+ -]")
DOC_SEPARATOR = re.compile(r"^---\s*$", re.MULTILINE)

# ---------------------------------------------------------------------------
# YAML block extraction
# ---------------------------------------------------------------------------


def extract_yaml_blocks(text: str) -> list[tuple[int, int, str]]:
    """Return a list of (start_line, end_line, yaml_content) from fenced code blocks.

    start_line is the first line of YAML content (1-indexed).
    end_line is the last line of YAML content (1-indexed, inclusive).
    """
    blocks: list[tuple[int, int, str]] = []
    lines = text.splitlines(keepends=True)

    inside = False
    start_line = 0
    buf: list[str] = []

    for i, line in enumerate(lines, start=1):
        if not inside and FENCE_OPEN.match(line.strip()):
            inside = True
            start_line = i + 1  # content begins on the next line
            buf = []
        elif inside and FENCE_CLOSE.match(line.strip()):
            inside = False
            end_line = i - 1  # last line of YAML content
            blocks.append((start_line, end_line, "".join(buf)))
        elif inside:
            buf.append(line)

    return blocks


def iter_file_blocks(
    files: list[Path], *, verbose: bool = False
) -> Iterator[tuple[Path, str, list[tuple[int, int, str]]]]:
    """Yield (path, file_text, blocks) for each file that has YAML blocks."""
    for path in files:
        if not path.exists():
            print(f"ERROR: {path} does not exist", file=sys.stderr)
            continue

        text = path.read_text()
        blocks = extract_yaml_blocks(text)
        if not blocks:
            if verbose:
                print(f"{path}: no YAML blocks found")
            continue

        yield path, text, blocks


# ---------------------------------------------------------------------------
# Diff-marker handling
# ---------------------------------------------------------------------------


def strip_diff_markers(content: str) -> str | None:
    """If every non-empty line starts with a diff prefix (+, -, or space),
    return the "after" state: context and added lines with the prefix stripped.
    Removed (-) lines are dropped.  Returns None if the block is not a diff."""
    lines = content.splitlines(keepends=True)
    if not lines:
        return None

    has_diff_marker = False
    for line in lines:
        if not line.strip():
            continue
        if not DIFF_LINE.match(line):
            return None
        if line[0] in ("+", "-"):
            has_diff_marker = True

    # A block where every line starts with a space is normal YAML, not a diff.
    if not has_diff_marker:
        return None

    after_lines: list[str] = []
    for line in lines:
        if not line.strip():
            after_lines.append(line)
        elif line[0] in (" ", "+"):
            after_lines.append(line[1:])
        # skip '-' lines (removed in the diff)
    return "".join(after_lines)


# ---------------------------------------------------------------------------
# Lint configuration & command
# ---------------------------------------------------------------------------

YAMLLINT_CONFIG = YamlLintConfig("extends: default")
YAMLLINT_RELAXED_CONFIG = YamlLintConfig(
    "extends: default\n"
    "rules:\n"
    "  document-start: disable\n"
    "  line-length: disable\n"
    "  comments:\n"
    "    min-spaces-from-content: 1\n"
)


def lint_block(
    yaml_content: str, *, strict: bool = False
) -> list[linter.LintProblem]:
    """Lint yaml_content and return a list of problems."""
    config = YAMLLINT_CONFIG if strict else YAMLLINT_RELAXED_CONFIG
    return list(linter.run(yaml_content, config))


def cmd_lint(args: argparse.Namespace) -> int:
    had_failure = False

    for path, _text, blocks in iter_file_blocks(args.files, verbose=args.verbose):
        for idx, (start_line, _end_line, content) in enumerate(blocks, start=1):
            lint_content = strip_diff_markers(content)
            is_diff = lint_content is not None
            if lint_content is None:
                lint_content = content

            problems = lint_block(lint_content, strict=args.strict)
            suffix = " (diff)" if is_diff else ""
            if problems:
                had_failure = True
                print(f"{path}: block {idx} (line {start_line}){suffix} FAILED")
                for p in problems:
                    md_line = p.line + start_line - 1
                    print(f"  line {md_line}:{p.column}: [{p.level}] {p.message}")
            elif args.verbose:
                print(f"{path}: block {idx} (line {start_line}){suffix} OK")

    return 1 if had_failure else 0


# ---------------------------------------------------------------------------
# Fix configuration & command
# ---------------------------------------------------------------------------

# yamlfix config tuned for YAML snippets inside markdown:
#   - no document-start marker (---)
#   - no line-length enforcement
#   - preserve existing quoting style
#   - 1 space before inline comments (matches YAMLLINT_RELAXED_CONFIG)
#   - keep existing sequence style (block vs flow)
YAMLFIX_CONFIG = YamlfixConfig(
    explicit_start=False,
    line_length=9999,
    comments_min_spaces_from_content=1,
    preserve_quotes=True,
    sequence_style=YamlNodeStyle.KEEP_STYLE,
)


def fix_yaml_content(content: str) -> str:
    """Fix YAML content, handling multi-document blocks by fixing each
    document separately and preserving --- separators."""
    parts = DOC_SEPARATOR.split(content)
    if len(parts) == 1:
        return fix_code(content, config=YAMLFIX_CONFIG)
    fixed_parts = [fix_code(part, config=YAMLFIX_CONFIG) for part in parts]
    return "---\n".join(fixed_parts)


def cmd_fix(args: argparse.Namespace) -> int:
    had_failure = False

    for path, text, blocks in iter_file_blocks(args.files, verbose=args.verbose):
        lines = text.splitlines(keepends=True)
        fixed_any = False

        # Process blocks in reverse so earlier line numbers stay valid
        # after replacing later blocks.
        for idx, (start_line, end_line, content) in reversed(
            list(enumerate(blocks, start=1))
        ):
            try:
                fixed = fix_yaml_content(content)
            except Exception as exc:
                had_failure = True
                print(
                    f"{path}: block {idx} (line {start_line}) SKIPPED"
                    f" — yamlfix could not parse: {exc}",
                    file=sys.stderr,
                )
                continue

            # Ensure the fixed content ends with a newline so the closing
            # fence stays on its own line.
            if fixed and not fixed.endswith("\n"):
                fixed += "\n"

            if fixed != content:
                fixed_any = True
                # Replace lines[start_line-1 : end_line] with the fixed content.
                fixed_lines = fixed.splitlines(keepends=True)
                lines[start_line - 1 : end_line] = fixed_lines
                print(f"{path}: block {idx} (line {start_line}) FIXED")
            elif args.verbose:
                print(f"{path}: block {idx} (line {start_line}) OK")

        if fixed_any:
            path.write_text("".join(lines))

    return 1 if had_failure else 0


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Lint and fix YAML code blocks inside markdown files."
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="Print OK status for passing blocks and files with no YAML blocks",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    # lint subcommand
    lint_parser = subparsers.add_parser(
        "lint", help="Lint YAML code blocks inside markdown files."
    )
    lint_parser.add_argument(
        "files", nargs="+", type=Path, help="Markdown files to check"
    )
    lint_parser.add_argument(
        "--strict",
        action="store_true",
        help="Use default yamllint rules (document-start, line-length, comments spacing enforced)",
    )
    lint_parser.set_defaults(func=cmd_lint)

    # fix subcommand
    fix_parser = subparsers.add_parser(
        "fix", help="Auto-fix YAML code blocks inside markdown files."
    )
    fix_parser.add_argument(
        "files", nargs="+", type=Path, help="Markdown files to fix"
    )
    fix_parser.set_defaults(func=cmd_fix)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
