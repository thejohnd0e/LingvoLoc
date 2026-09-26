# Agent Instructions

This is the main shared instruction file for Codex, Claude Code, OpenCode, and other coding agents working in this repository.

## Required Context

Before substantial work, read:

1. `AGENTS.md`
2. `STATUS.md`
3. `DECISIONS.md`
4. `TODO.md`

Inspect the relevant code and configuration before making changes. Do not invent project facts; record unknown information as `TBD`.

## Working Guidelines

- Make the smallest correct change that satisfies the task.
- Preserve established project conventions once they exist.
- Do not overwrite or revert unrelated work.
- Keep documentation concise and avoid duplicating information across files.
- Add or update tests when behavior changes, once a test framework exists.
- Run relevant build, test, lint, and formatting checks when available.
- Report checks that could not be run and why.

## Documentation Maintenance

After substantial work, update the following when relevant:

- `README.md` for stable setup, usage, build, or test instructions.
- `STATUS.md` for current capabilities, active work, known issues, and the immediate next step.
- `DECISIONS.md` for significant technical or architectural choices and their rationale.
- `TODO.md` for remaining actionable work; remove completed items.

Do not use these files as detailed activity logs.
