# Agent Guidelines

## Overview

This repository uses industry best practices for documentation, code quality, and observability. Follow these instructions when contributing changes with the Codex agent.

## Commit Messages

- Use conventional commits: `<type>(<scope>): <subject>`.
- Reference the GitHub issue ID in the description, e.g. `Refs #1`.
- Keep the subject under 72 characters.
- Provide a body when the change is non-trivial.

## Documentation

- All Markdown should pass markdownlint.
- Update or create guides under `docs/guides/` when introducing new processes or workflows.
- Link new guides from `README.md` when relevant.

## Test Driven Development

- Favor adding tests alongside changes. When modifying Python code, run `make test-python`.
- Use mocking to shift testing left and enable faster feedback loops.
- Provide debugging tips in documentation when applicable.

## Observability

- Ensure logging remains consistent and easy to parse.
- Prefer structured logs and metrics that aid production debugging.

