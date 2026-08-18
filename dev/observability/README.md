# Local Langfuse

This directory runs Langfuse v4 locally for Agent development. The Compose file is based on Langfuse's official local deployment and binds PostgreSQL to host port `5433` because this repository's development machine may already use `5432`.

## Start

```sh
./dev/observability/setup.sh
```

The script creates the ignored `dev/observability/.env` on first run, starts Langfuse, prints the local login, and prints the `[observability]` block to add to `~/.agent/config.toml`. All services use `restart: "no"`, so they do not start automatically when Docker starts; they run only after this script (or an explicit `docker compose up`) is invoked.

Open <http://localhost:3000>. The initialized project is **Agent Development**.

## Verify ingestion

After enabling observability in `~/.agent/config.toml`:

```sh
cargo run -p agent-cli -- observability-check
```

The `observability.check` trace should appear in Langfuse. Each subsequent Agent turn creates an `agent.run` trace grouped under the configured Agent session.

## What is captured

- Root turn input and final output
- Complete provider request: instructions, canonical context, tool definitions, and compaction threshold
- Model output items, token usage, and provider-reported cached input tokens
- Discovered skill metadata and the exact skill catalog text injected into instructions
- Tool call arguments, results, denials, and errors
- Server-side compaction input/output and before/after item counts
- Latency, parent/child relationships, environment, and session ID

Prompt, skill, tool, and model content is intentionally captured in full because this deployment is only for local development. Do not point this configuration at a shared Langfuse instance without adding an appropriate masking policy.

## Stop

```sh
docker compose \
  --env-file dev/observability/.env \
  -f dev/observability/docker-compose.yml \
  down
```

Add `-v` only when you intentionally want to delete all local Langfuse data.

## Upgrade

The images currently follow the Langfuse v4 major tag. Pull and recreate them with:

```sh
docker compose \
  --env-file dev/observability/.env \
  -f dev/observability/docker-compose.yml \
  pull
./dev/observability/setup.sh
```
