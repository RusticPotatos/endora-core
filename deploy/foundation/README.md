Status: EXPERIMENTAL · container recipe, built on one Linux host, not deployed

# endora-foundation in a container

A recipe. It was built on a Linux test host (118 MB image, both base images pinned by digest) and its
healthcheck and the smoke script passed in it; it has **not** been deployed as a service. Check it
before relying on it (status: EXPERIMENTAL, see `docs/foundation/STABILITY.md`).

- `Dockerfile`: builds `endora-foundation-cli` (`--release --locked`, Rust 1.87) and ships the
  one binary on `debian:bookworm-slim`, as a non-root user, with a volume at `/data`.
  Both base images are pinned by digest in the file.
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
