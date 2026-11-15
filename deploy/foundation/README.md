Status: EXPERIMENTAL · container recipe; built and run on a Linux host and again on an Apple-silicon Mac, not deployed as a service

# endora-foundation in a container

A recipe. It was built on a Linux test host (118 MB image, both base images pinned by digest) and its
healthcheck and the smoke script passed in it. It was built and run again on 15 Nov 2026 on an
Apple-silicon Mac with Docker 29 (100 MB image, `arm64`): the steps under "Run" below were followed as
written (in a throwaway compose project with its own image tag and volume, all removed afterwards): the
owner credential, a goal and a routine, a dated backup copied out and a restore from it. The service stayed
`healthy` and kept its state across a restart. It has **not** been deployed as a service. Check it before
relying on it (status: EXPERIMENTAL, see `docs/foundation/STABILITY.md`).

- `Dockerfile`: builds `endora-foundation-cli` (`--release --locked`, Rust 1.87) and ships the
  one binary on `debian:bookworm-slim`, as a non-root user (uid 10001), with a volume at `/data`.
  Both base images are pinned by digest in the file.
- `compose.example.yml`: one service, no published ports, a named volume for the database and a
  read-only bind of your config, `restart: unless-stopped`, a read-only root file system and a `/tmp`
  that is a tmpfs.

## Run

All commands are run from this directory. Below, `dc` stands for
`docker compose -f compose.example.yml`.

1. Write `foundation.conf` beside the compose file, one probe per line, for example
   `probe web tcp 127.0.0.1:8080 60` (the line format is in `docs/foundation/README.md`). Keep it out of
   version control. The address must be one the **container** can reach (`127.0.0.1` is the container
   itself); a reserved documentation address such as `192.0.2.10` never answers, so its probe fails on
   every firing and, after five failures in a row, pauses the routine.
2. `dc up -d --build`
3. The container runs `serve --every 60`: `run-due` every minute, until SIGTERM.
4. **Make the owner credential, once.** Nothing works as the owner until you do:
   `dc exec foundation endora-foundation init --token-out /data/owner.token`
   Then every owner command needs the token file, so put `--token-file /data/owner.token` before the
   command:
   `dc exec foundation endora-foundation --token-file /data/owner.token goal add --title Web --probe web`
   (to avoid typing it, add `ENDORA_OWNER_TOKEN_FILE=/data/owner.token` under `environment:` in the
   compose file; it is a path, never the token itself). Without it an owner command exits 2 with
   `this command needs the owner token`. `status`, `evidence`, `verify-evidence`, `stop` and `backup` do not
   need it.
5. Use the command line inside it against the same file, for example
   `dc exec foundation endora-foundation status`, then `routine add`, `accept`, `stop`, each with the
   token file for the owner commands (the full list is in `docs/foundation/README.md`).

### What the token's place means

The root file system is read-only and `/tmp` is a tmpfs, so the volume at `/data` is the only place a
file survives a restart. The token therefore lives **on the same volume as the database** (mode 600, owned
by the container user). Anyone who can read the volume (the host's root user, anything that mounts it)
holds the owner credential. A token written anywhere else (`/tmp`, say) is gone at the next restart, and
**a lost token cannot be replaced**: `init` refuses to run on a database that already has a credential
(exit 2) and no command makes a new one. Copy `owner.token` off the volume once, with the same care as
the database (see "Back up", below, and "Known limits" in `docs/foundation/README.md`).

### Back up

`backup --to` makes a consistent snapshot while the service runs, and **never overwrites a file**: running
it twice with the same name fails (`already exists; a snapshot never overwrites an earlier file`, exit 1).
Use a dated name, then copy it **off the volume**: a copy that stays in `/data` is lost with the volume
it is meant to protect.

```sh
dc exec foundation endora-foundation backup --to /data/backup-$(date +%F).db
dc cp foundation:/data/backup-$(date +%F).db ./          # to the host; then to another disk
dc cp foundation:/data/owner.token ./                    # once; the backup does not contain the token
dc exec foundation rm /data/backup-$(date +%F).db        # drop the copy on the volume when it is safe elsewhere
```

The backup holds the database only: the evidence, goals and a salted hash of the token, **not** the token
file and not the config file. Note the `head` hash that `verify-evidence` prints and keep it with the copy.

### Restore

Stop the service, copy the backup over the database in the volume, start it again. Work done after the
backup is gone, and goal and item numbers are issued again from where the backup stood.

```sh
dc stop
dc run --rm --no-deps -v "$PWD:/restore:ro" --entrypoint cp foundation \
   /restore/backup-2026-11-15.db /data/foundation.db
dc up -d
dc exec foundation endora-foundation verify-evidence     # compare the head hash you kept
```

Do this with the **same or an older** binary than made the backup, or read "Back up before you upgrade"
in `docs/operate.md`: a newer binary upgrades any database file it opens.

## Notes

- The healthcheck runs `endora-foundation status`; it only proves the file opens. It cannot tell you that
  a routine is paused or an item failed: read `status` for that (`docs/operate.md`).
- `stop` is the switch: while on, `serve` keeps running but does no work. Anyone may run it; `resume`
  needs the token.
- A `git` config line needs `git` and a sandbox in the image; this image has neither, so
  `git-task` is not available here. Probes of kind `tcp`, `http` and `command` are.
- Upgrading the image: stop the old container (and any `git-task` or cron `run-due` on the same file)
  before the new one starts. A process of a build before schema 8 that is still running when the new
  build first runs has its running item recovered as if it had died.
- Back up with `backup --to`, not by copying the live file.
- The image holds no secrets; the config is mounted, never baked in.
- Limits you will meet: a goal or a routine cannot be retired and a lost owner token cannot be replaced;
  see "Known limits" in `docs/foundation/README.md`.
