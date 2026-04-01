---
name: xcom-rs
description: Interact with X/Twitter via the xcom-rs CLI (Rust). Use for posting tweets, replies, threads, searching, reading timelines/mentions, liking, retweeting, bookmarks, media upload, and user lookups. Use this skill whenever the user wants to do anything on X/Twitter — posting, reading, searching, monitoring mentions, managing bookmarks, or looking up users and their tweets.
version: 1.0.0
author: tumf
license: MIT
platforms: [linux, macos]
prerequisites:
  commands: [xcom-rs]
metadata:
  hermes:
    tags: [twitter, x, social-media, xcom-rs]
    homepage: https://github.com/tumf/xcom-rs
---

# xcom-rs — X/Twitter CLI

Agent-friendly Rust CLI for X/Twitter with structured output, OAuth2 PKCE auth, cost tracking, and idempotency support.

## Install

```bash
cargo install --git https://github.com/tumf/xcom-rs.git
```

Verify:

```bash
xcom-rs --help
xcom-rs doctor --output json
```

## Authentication

xcom-rs uses OAuth2.0 PKCE (preferred) or OAuth1.0a 3-legged flow. No manual API key juggling needed.

```bash
xcom-rs auth login          # Interactive browser-based OAuth2 PKCE
xcom-rs auth status          # Check current auth state
xcom-rs auth logout          # Revoke and clear tokens
```

Alternatively, set `XCOM_RS_BEARER_TOKEN` env var for app-only (read) access.

Check health with:

```bash
xcom-rs doctor --output json
```

This shows auth status, granted scopes, budget storage, and can probe the API with `--probe`.

## Output Formats

All commands support `--output json|yaml|text`. Use `--output json` when parsing results programmatically. All JSON responses follow a consistent envelope:

```json
{"ok": true, "type": "<command>", "schemaVersion": 1, "data": { ... }}
```

On error:

```json
{"ok": false, "error": {"code": "RATE_LIMIT_EXCEEDED", "message": "...", "retryable": true}}
```

## Commands Reference

### Tweets

```bash
# Show a tweet
xcom-rs tweets show <tweet_id> --output json

# Get conversation thread for a tweet
xcom-rs tweets conversation <tweet_id> --output json

# Reply to a tweet
xcom-rs tweets reply <tweet_id> "reply text" --output json

# Post a thread (sequential replies)
xcom-rs tweets thread "first" "second" "third" --output json

# Like / unlike
xcom-rs tweets like <tweet_id>
xcom-rs tweets unlike <tweet_id>

# Retweet / unretweet
xcom-rs tweets retweet <tweet_id>
xcom-rs tweets unretweet <tweet_id>
```

### Search

```bash
# Search recent tweets
xcom-rs search recent "query" --limit 10 --output json

# Search users
xcom-rs search users "query" --limit 10 --output json
```

### Timelines

```bash
# Home timeline
xcom-rs timeline home --limit 20 --output json

# User's tweets
xcom-rs timeline user <username> --limit 20 --output json

# Mentions
xcom-rs timeline mentions --limit 20 --output json
```

### Bookmarks

```bash
xcom-rs bookmarks list --limit 20 --output json
xcom-rs bookmarks add <tweet_id>
xcom-rs bookmarks remove <tweet_id>
```

### Media

```bash
# Upload media (returns media_id for attaching to tweets)
xcom-rs media upload <file_path> --output json
```

### Introspection

```bash
# List all commands with metadata (risk level, cost info)
xcom-rs commands --output json

# Get JSON schema for a command's input/output
xcom-rs schema <command_name> --output json

# Get detailed help including exit codes and examples
xcom-rs help <command_name> --output json
```

## Idempotency

Write commands support `--client-request-id <id>` (or `--client-request-id-prefix <prefix>` for threads) to prevent duplicate posts on retry. Use `--if-exists return|error` to control behavior when a duplicate is detected.

```bash
xcom-rs tweets reply 123 "hello" --client-request-id my-reply-001 --if-exists return
```

## Cost & Budget Controls

xcom-rs tracks API cost in credits. Control spending with:

```bash
--max-cost-credits <N>       # Fail if single operation exceeds N credits
--budget-daily-credits <N>   # Fail if daily total would exceed N credits
--dry-run                    # Estimate cost without executing
```

Budget data stored at `~/.local/share/xcom-rs/budget.json`.

## Non-Interactive Mode

For agent use, always pass `--non-interactive` to prevent prompts:

```bash
xcom-rs tweets reply 123 "hello" --non-interactive --output json
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 2 | Invalid/missing argument |
| 3 | Auth/authorization failure |
| 4 | Operation failed (network, rate limit, etc.) |

## Error Codes

Retryable: `RATE_LIMIT_EXCEEDED`, `NETWORK_ERROR`, `SERVICE_UNAVAILABLE`
Not retryable: `INVALID_ARGUMENT`, `MISSING_ARGUMENT`, `AUTHENTICATION_FAILED`, `AUTHORIZATION_FAILED`

## Agent Workflow

1. Check auth: `xcom-rs auth status --output json`
2. If not authenticated: `xcom-rs auth login` (needs interactive) or set `XCOM_RS_BEARER_TOKEN`
3. Use `--output json --non-interactive` for all commands
4. Parse the `ok` field to check success
5. For writes (reply, thread, retweet), confirm user intent first — these are public actions
6. Use `--client-request-id` for idempotent retries on write operations

## Pitfalls

- **Auth required**: Most commands need OAuth2 login first. `doctor` will tell you what's missing.
- **Rate limits**: X API has per-endpoint rate limits. Retryable errors include `RATE_LIMIT_EXCEEDED`.
- **Tweet posting**: `tweets reply` and `tweets thread` are the write commands. There is no standalone `tweet post` — use `tweets thread "single tweet"` for a single post.
- **Scopes**: If writes fail, check `doctor` output for missing scopes (need `tweet.write`, `like.write`, etc.).
- **Cost tracking**: xcom-rs tracks credits locally. Set `--budget-daily-credits` to prevent runaway usage.
