# Working in this repository

Public engine and CLI. Keep private implementation and private Git history out of this repository.

Prefer a git worktree per task, so parallel agents never share a working tree;
merge finished work onto `main` and remove the worktree. Commit changes in the
repository that owns them.
Build outputs stay in ignored directories; never commit credentials or databases.
