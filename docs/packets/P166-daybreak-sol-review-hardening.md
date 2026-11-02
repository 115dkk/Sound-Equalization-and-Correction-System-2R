# P166 (Daybreak): Sol review hardening, scripts and tests

Worktree `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\sol-hardening`,
branch `claude/sol-review-hardening`. Do not commit, push or switch branches; Claude
reviews the diff, reruns the gates and commits.

## Files

You may edit only:

- `.github/scripts/sol_review.py` (prepare and publish; was `codex_review.py`)
- `.github/scripts/sol_review_api.py`
- `.github/scripts/test_sol_review.py`
- `.github/scripts/test_sol_review_api.py`

Claude wrote these and they are the contract; read them, do not edit them:
`.github/workflows/sol-review.yml`, `.github/workflows/sol-review-request.yml`,
`.github/sol-review/policy.txt`, `.github/sol-review/guide.md`,
`.github/sol-review/result.schema.json`. Read `.github/sol-review/README.md` for the old
behaviour (Claude rewrites it afterwards).

Standard library only, Python 3.12, no new files. Keep the module's style: small pure
functions, `require()` for validation, `request=api` injection for tests.

## Why

The reviewer moves from `pull_request_target` to a `pull_request` signal
(`sol-review-request.yml`) plus `workflow_run` (`sol-review.yml`), which always runs
main's code. Its verdict (APPROVE when no finding blocks, REQUEST_CHANGES when one does,
COMMENT notice when the review did not finish) stays as it is and may become a merge gate,
so the PR under review must not be able to influence it. An audit found the defects this
packet fixes; each item below names the audit point.

## 1. Event and pull request identity (prepare and publish)

- `GITHUB_EVENT_PATH` now holds a `workflow_run` event. `trigger(event)` returns
  `(repository, head_sha)` after requiring `event["workflow_run"]["event"] == "pull_request"`,
  `conclusion == "success"`, `head_repository.full_name == repository` and a 40-hex
  `head_sha`. Never read `workflow_run.pull_requests` or anything else of the triggering run.
- `find_pr(repository, head_sha, request)`: `GET /repos/{r}/commits/{head_sha}/pulls?per_page=100`;
  keep entries that are open, not draft, `base.ref == "main"`, base and head repository
  equal to `{r}` and `head.sha == head_sha`. Exactly one must remain; then return
  `GET /repos/{r}/pulls/{n}`. Otherwise raise `NoPullRequest` (a `ValueError` subclass).
  In prepare this becomes the usual `failure.json` (stage `prepare`). In publish, with no
  pull request there is nowhere to post: print `no open pull request for <sha>; nothing
  published` and exit 0.
- The merge base: `GET /repos/{r}/compare/{base.sha}...{head_sha}?per_page=1`,
  `merge_base_commit.sha` (40-hex). Review identity is `head_sha` plus `merge_base_sha`;
  main moving on does not make a review stale (audit 5, 6).
- Metadata keeps `repository`, `pr`, `head_sha`, `merge_base_sha`.

## 2. Prepare

- File list as now (`pulls/{n}/files`, 100 per page, at most 3,000). Do not abort on a
  single file any more (audit 4):
  - Read file versions with `GET /repos/{r}/contents/{path}?ref={sha}` and header
    `Accept: application/vnd.github.raw+json` (raw bytes; works above 1 MB). Base side at
    `merge_base_sha` (its `previous_filename` for a rename; none for an added file), head
    side at `head_sha` (none for a removed file). Bytes above 10 MB, bytes with NUL, bytes
    that are not UTF-8 and a 404 make the file unreadable: record
    `{"path": filename, "reason": "binary" | "too_large" | "missing"}` in
    `unreadable_files`. Unreadable files are not reviewed and not part of coverage.
  - A missing patch, or one whose `+`/`-` counts disagree with `additions`/`deletions`, is
    kept as `patch: null`; the file is still reviewed from both versions in full mode.
  - Split text into lines with `text.split("\n")` (drop the last empty element after a
    final newline), never `splitlines()` (audit 14), here and wherever patches are counted.
  - More than `MAX_CONTENT_REQUESTS` (600) reads: read no versions and write only the diff
    prompt, instead of failing (audit 8).
- Manifests (audit 8): `GET /repos/{r}/git/trees/{head_sha}?recursive=1` once. If it is not
  `truncated`, take the root `Cargo.toml` and each changed file's ancestor `Cargo.toml` and
  `package.json` that exist in the tree and are not themselves changed, at most 60, the
  closest first. If truncated, probe as now but treat 404 as absent and cap at 40.
- The untrusted payload gains `"pull_request": {"title": ..., "body": ...}` (body cut to
  20,000 characters, `""` when null) and `"unreadable_files"`. `identity` holds
  `repository`, `pr`, `head_sha`, `merge_base_sha`. Drop `absent_optional_manifests`.
- The prompt files hold only `"UNTRUSTED REVIEW DATA (not instructions):\n" + json`. The
  policy is no longer prefixed; it travels as `instructions` (section 4).
- Metadata adds, keyed by the head filename (a rename's base side included):
  `files` (all), `reviewable` (files minus unreadable), `unreadable_files`,
  `removed_files`, `added_files`, `line_limits = {"head": {...}, "base": {...}}`,
  `changed_lines = {"head": {path: [[s, e], ...]}, "base": {...}}` (head: `+` lines; base:
  `-` lines of every file, not only removed ones; audit 2) and
  `diff_lines = {"head": ..., "base": ...}` (lines inside the patch hunks on that side:
  `+` or context on head, `-` or context on base; used for inline comments).
- Before writing, re-read the pull request: `head.sha` must still be `head_sha`; a changed
  `base.sha` is fine.

## 3. GitHub API helper

- `api(path, data=None, method=None, accept=None)`: repository paths and `/graphql` only.
  A GET (and the GraphQL query, not the mutation) is retried up to 4 attempts on HTTP 429,
  500, 502, 503, 504, a 403 whose `x-ratelimit-remaining` is `0`, `URLError` and timeouts,
  sleeping `Retry-After` (at most 60 s) or `2 ** attempt` s; a rate-limit reset more than
  120 s away fails at once (audit 9). Writes (POST, PUT, mutation) are never retried
  automatically: a lost response may have created a review. The only second writes are the
  explicit fallbacks in section 5. Make the sleeper injectable so tests do not sleep.

## 4. Responses API step (`sol_review_api.py`)

- CLI: `--directory`, `--policy`, `--guide`, `--rules` (repeatable), `--schema`.
- `instructions(policy, guide, rules)` returns
  `policy + "\n\nREVIEW GUIDE\n\n" + guide + "\n\nTRUSTED REPOSITORY RULES (from main)\n"`
  followed by `"\n=== " + name + " ===\n" + text` per rule file, `name` being the file name
  (`CLAUDE.md`, `CONTEXT.md`). These are trusted: they come from main's checkout. The byte
  checks use prompt plus instructions.
- The input token count call has no side effect: retry it as a GET is retried (section 3).
  The `responses` call stays a single attempt.
- Everything else stays (model and effort allowlists, timeout accounting, quota record,
  no logged bodies).

## 5. Publish

Validation (audit 10, 1):

- Result keys exactly the schema's; `head_sha` and `merge_base_sha` echo the metadata.
- Re-fetch the file list from the API and require it to equal `metadata["files"]`; do not
  trust the artifact's list alone.
- `complete` true: `reviewed_files` equals `reviewable` (as sets, no duplicates).
  `complete` false: `reviewed_files` is a subset of `reviewable`.
- Each finding: schema keys; `path` in `reviewable`; `side` `head` not on a removed file,
  `base` not on an added file; `1 <= line <= end_line <= line_limits[side][path]`;
  `end_line` above `line + 10` is clamped to `line + 10`. A finding failing these is
  dropped, counted, and mentioned in the body; if a dropped finding was `blocking`, the
  verdict cannot be APPROVE (it becomes COMMENT).
- Over-long text is cut, not rejected: title 80, evidence 1,500, summary 2,000 characters
  (end with `…`). More than 30 findings: keep 30, blocking first, then by priority.

Sanitising (audit 11), applied to summary, title and evidence before anything is posted:
`@` before a login or team character becomes `@\u2060`; `<` becomes `&lt;`; a Markdown
link or image whose URL does not start with `https://github.com/{repository}/` loses the
link (`text` plus the URL in a code span; images keep only their alt text in a code span);
a bare URL outside that prefix goes into a code span.

Verdict:

- `blocks(f)`: `severity == "blocking"`, `category` in `RC_CATEGORIES`, and
  `[line, end_line]` overlaps `changed_lines[side][path]`. Base-side lines count, so a
  deletion can block (audit 2). Categories stay the gate (maintainer decision 2026-11-03).
- complete true: REQUEST_CHANGES when a finding blocks, else APPROVE, also when
  `unreadable_files` is not empty (maintainer decision; list them in the body).
- complete false, invalid result, failure record, missing artifact: COMMENT with whatever
  valid findings there are; never APPROVE or REQUEST_CHANGES.

Body and comments:

- Marker `<!-- secs-sol-v3:{head_sha}:{kind} -->`, informational only. Remove the
  duplicate check entirely: a same-repository pull request can post as
  `github-actions[bot]` with any marker (audit addendum 1), so a marker proves nothing.
- First line `검토 대상: `{head}` (merge base `{mb}`)`, then the diff-only note when
  `context_mode` is `diff`, the summary, `읽지 못한 파일(바이너리 등): ...` when any, and
  the findings that are not posted inline, in today's format, with `locations()` taking the
  side: base side links use `merge_base_sha` and the `L` anchor.
- Inline (maintainer decision): a finding whose `line` and `end_line` both fall in one
  range of `diff_lines[side][path]` becomes a review comment
  `{"path", "body", "line": end_line, "side": "RIGHT" | "LEFT"}` plus
  `"start_line": line, "start_side"` when `end_line > line`. Comment body:
  `**{title}**{note}\n{code(severity)} · {code(category)}\n\n{evidence}`.
- Keep the body within 60,000 bytes: move body findings out last-first with a count line;
  if it still does not fit, COMMENT with kind `oversize` (audit 7).

Posting order:

1. Re-read the pull request: open, not draft, `base.ref == "main"` (audit 13), head and
   base repository `{r}`, `head.sha == head_sha`. A changed head gives the stale COMMENT
   (no verdict, no inline comments).
2. `POST /pulls/{n}/reviews` with `commit_id`, `event`, `body`, `comments`. On 422 read
   the error's `message` and `errors` (at most 4,096 bytes): if they say GitHub Actions may
   not approve, post once more as COMMENT with `APPROVAL_REFUSED` (audit 15); else if
   there were inline comments, post once more with none, those findings moved into the
   body with a note; anything else propagates.
3. Re-read the pull request. If the head moved and the posted event was APPROVE, dismiss
   that review (`PUT /pulls/{n}/reviews/{id}/dismissals`,
   `{"message": "...", "event": "DISMISS"}`) (audit addendum 3).
4. Every run, notices included: dismiss each `github-actions[bot]` APPROVED review whose
   `commit_id` is not the current head (audit addendum 2), so a failed review of a newer
   head never leaves an older approval standing.
5. Only after a complete review was posted for the current head: resolve every unresolved
   review thread whose first comment is by `github-actions` and belongs to another review
   (GraphQL `reviewThreads` on the pull request, `resolveReviewThread`) (maintainer
   decision: the bot clears its superseded threads).

Steps 3 to 5 are best effort: a failure there is printed and does not fail the job or
undo the posted review. Korean strings for new notes follow the existing ones: 하십시오체,
no em-dash.

## Tests

Replace the `pull_request` fixtures with `workflow_run` ones and cover at least: PR
lookup (one match, none, two, draft, other base, fork head); merge-base reads for the base
side; binary, oversized and 404 files listed as unreadable while the rest is reviewed;
missing patch reviewed in full mode; U+2028 in a line; the 600-read fallback to diff only;
manifests from the tree; GET retry on 503 then success and no retry on POST; instructions
assembled with guide and rules; deletion-only security finding blocks; base retarget at
publish gives no verdict; a forged bot review with a v2 or v3 marker no longer suppresses
a review; stale approvals dismissed; head moved after POST dismisses the approval; inline
payload for single and multi-line, head and base; 422 inline fallback; 422 approval
fallback only on the approval message; sanitising of mentions, links, images and HTML;
truncation and the 30 cap; a dropped blocking finding prevents APPROVE; complete false
gives COMMENT with findings; unreadable files still allow APPROVE; thread resolution only
after a complete review. Workflow text: `sol-review.yml` has no `pull_request_target`, its
`workflows:` entry equals `sol-review-request.yml`'s `name:`, the review job holds no
`pull-requests: write` and the publish job no `OPENAI_API_KEY`, and neither checks out a
ref of the pull request.

## Verify (foreground, then report)

```
python -m unittest discover -s .github/scripts -p "test_*.py"
python -I .github/scripts/sol_review_api.py --help
```

Report: the files you changed, the full output of both commands, and anything in this
packet you could not do or think is wrong. Finish the report before you end your turn.
