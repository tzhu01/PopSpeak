# Source release snapshot audit

`scripts/audit-source-release.ps1` is a read-only guard for preparing a source
release. It does not delete files, stage changes, rewrite commits or print matched
secret values.

## What it checks

- suspicious paths in the tracked worktree, one selected publish commit and
  untracked non-ignored files,
  including local `.env`/settings files, databases, private keys/keystores,
  archives and audio captures;
- private-key markers in tracked and untracked non-ignored content;
- optionally, exact known leaked values supplied in a local denylist;
- optionally, whether the worktree is clean.

The report contains only a category, scan scope and repository path. It never
prints the matching line or denylist value.

Untracked enumeration uses `git ls-files --others --exclude-standard`, so files
excluded by `.gitignore` are not opened. In particular, downloaded models,
`target/`, `node_modules/` and normal build output remain outside this audit.
An untracked file larger than 16 MiB, a link, or an unreadable file is reported
by path for manual review rather than followed or silently skipped.

## Usage

Run the snapshot check for the commit you intend to publish:

```powershell
./scripts/audit-source-release.ps1 -PublishRef HEAD
```

Require a clean worktree for final release rehearsal:

```powershell
./scripts/audit-source-release.ps1 -PublishRef HEAD -RequireCleanWorktree
```

To detect a credential that is already known to have leaked, create a local text
file **outside the repository** with one literal value per line. Blank lines and
lines beginning with `#` are ignored; each value must contain at least eight
characters. Then run:

```powershell
./scripts/audit-source-release.ps1 `
  -PublishRef HEAD `
  -DenylistPath "$HOME/.popspeak-release-denylist.txt"
```

Do not commit or attach that denylist. The script sends its values to `git grep`
over standard input, requests path-only output and never places a value on the
Git command line.

## Important boundary: snapshot is not history

Passing this script means only that the tracked worktree, selected commit
snapshot and current untracked non-ignored files passed these targeted checks.
A deleted secret can still exist in an older commit, tag, remote-tracking branch,
stash, ignored local file or local checkpoint ref.

Before publishing:

1. rotate/revoke every credential that has ever left its intended secret store;
2. enumerate the exact branches and tags that will be pushed (never use
   `git push --all` or `git push --mirror` from a development clone);
3. run the current official [Gitleaks](https://github.com/gitleaks/gitleaks)
   Git-history scan over every ref that will be published;
4. review every finding before creating a clean public mirror or release tag.

This project intentionally does not provide an automatic history-rewrite command.
History rewriting is destructive and requires an explicit, separately reviewed
remediation plan.
