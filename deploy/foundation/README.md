# endora-foundation in a container

A recipe, **written but not built or run** where it was authored. Check it before relying on it.

- `Dockerfile`: builds `endora-foundation-cli` (`--release --locked`, Rust 1.87) and ships the
  one binary on `debian:bookworm-slim`, as a non-root user, with a volume at `/data`.
  Pin both base images by digest first (the root `Dockerfile` shows how).
- `compose.example.yml`: one service, no published ports, a named volume for the database and a
  read-only bind of your config, `restart: unless-stopped`.

## Run

1. Write `foundation.conf` beside the compose file, for example `probe web tcp 192.0.2.10:443 60`
   (the line format is in `docs/foundation/README.md`). Keep it out of version control.
2. `docker compose -f compose.example.yml up -d --build`
3. The container runs `serve --every 60`: `run-due` every minute, until SIGTERM.
4. Use the command line inside it, against the same file:
   `docker compose -f compose.example.yml exec foundation endora-foundation status`
   (also `goal add`, `routine add`, `accept`, `stop`, `backup --to /data/backup.db`).

## Notes

- The healthcheck runs `endora-foundation status`; it fails if the file cannot be opened.
- `stop` is the owner's switch: while on, `serve` keeps running but does no work.
- A `git` config line needs `git` and a sandbox in the image; this image has neither, so
  `git-task` is not available here. Probes of kind `tcp`, `http` and `command` are.
- Back up with `backup --to` (a consistent snapshot while running), not by copying the live file.
- The image holds no secrets; the config is mounted, never baked in.
