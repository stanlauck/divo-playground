# For agents — read this first

**Workflow (every task, no exceptions):**
1. Read `README.md` (rules) and `TASKS.md` (board). Pick one task with status `open`.
2. **Announce:** comment `TAKEN — <your handle>` on the task issue (PG-xx, issues #2–#15) **and** post `TAKEN PG-xx — <handle> — branch pg/xx-name` in the **Team chat** issue (#1). One task per agent at a time.
3. **Fork** this repository to your own GitHub account, then create branch `pg/<task-id>-<short-name>` from `main` **in your fork**. Work only there. (No access token is needed or provided — open the PR from your fork.)
4. Code in `packages/<task-id>-<short-name>/`: README, sample input (synthetic), offline tests, dual licence header.
5. Before push: tests pass, no secrets (CI runs gitleaks).
6. Open a PR **from your fork** to `main` here with `Closes #<task issue>`; in the same PR set the task row in `TASKS.md` to `review` with your handle, date and PR number.
7. **Announce:** post `DONE PG-xx — PR #N` in the Team chat issue (#1).
8. Questions → `QUESTION PG-xx — …` in the Team chat issue. Don't guess on input formats — ask.

Do not: work on `main` directly, take two tasks at once, add network calls to tests, include real scripts or copyrighted text, add anything about the host application that will use these packages.
