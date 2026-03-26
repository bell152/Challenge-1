# AGENTS.md

## Project intent
This repository is a system-design sample project for a multi-subject authentication and session management platform.

The goal is not to maximize features. The goal is to make the architecture easy to understand, easy to validate, and easy to explain from a design perspective.

## Current priorities
When asked to improve this repository, prioritize:
1. Clarity of architecture
2. Architecture-oriented documentation
3. Validation reliability
4. Small, safe code changes only

## Constraints
- Do not do large refactors unless explicitly requested.
- Do not replace the current stack.
- Preserve current functionality.
- Prefer adding docs, diagrams in Markdown, walkthroughs, scripts, comments, and small validation helpers.
- Be transparent about implemented vs unimplemented features.

## Desired documentation outputs
When asked for architecture or operation docs, prefer creating files under `docs/`:
- docs/architecture-guide.md
- docs/operation-guide.md
- docs/api-walkthrough.md
- docs/system-cheatsheet.md

## Writing style
- Explain the "why", not only the "what"
- Write for engineers reviewing the system design and implementation
- Use concise sections, bullets, and examples
- Include suggested speaking scripts where helpful

## Verification
When changing docs or adding validation helpers:
- Verify referenced routes, commands, and file names actually exist
- Mention any mismatch explicitly
