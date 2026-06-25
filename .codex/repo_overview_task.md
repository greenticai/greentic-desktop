# Repo Overview Maintenance

Maintain a single Markdown file at `.codex/repo_overview.md`.

The file must include:

1. A concise high-level overview of what the repo does.
2. A breakdown of main components/modules and their current functionality.
3. A clear list of WIP/TODO/stub areas.
4. Broken, failing, or conflicting areas, including test/build failures and explicit broken/HACK/TEMP comments.
5. Notes for future work implied by the current state.

When refreshing the overview:

- Scan the top-level structure and build/config files.
- Inspect entrypoints such as `src/lib.rs`, tests, benches, workflows, and scripts.
- Search for markers such as `TODO`, `FIXME`, `XXX`, `HACK`, `TEMP`, `BROKEN`, `unimplemented`, and `todo!`.
- Run the repo's standard validation command when practical: `bash ci/local_check.sh`.
- Fully replace stale content rather than appending conflicting information.
- Keep wording factual, concise, and grounded in the current repo.
