# Core ↔ Console contract, v1

What Endora Console may ask of Endora Core, and how Core answers. The machine-readable list is
[`console-v1.json`](console-v1.json): every route Console may call, by method and path, and
nothing else. Core's tests hold every listed route to being served by the node alone; Console's
proxy passes exactly this list and its checks hold the screens to it
([ADR 0145](../adr/0145-core-console-contract.md)).

## Rules

- **One version, additive.** `/v1` gains routes and fields; none is removed, renamed or given a
  new meaning, and an event kind is never reused. Console ignores fields it does not know.
  Core says its contract version at `/health` as `api` (`"1.0"`; the minor number grows with
  each addition). Console refuses a Core whose major version it does not know, with a plain
  message, and tests only the current pair.
- **Core authorizes every write.** Console holds no credential and no session store. The
  person's session token (from signing in to Core) travels in `Authorization: Bearer` from the
  browser, through Console's proxy, to Core, which checks it and its scope on every request.
- **Console never calls HQ.** The engine's own routes (port 8790, `/v1/runner/*`, `/v1/team/*`,
  `/v1/round`, `/v1/projects`, `/v1/tasks`, `/v1/goals`, `/v1/members`, `/v1/pool`) are the
  research rig's and the workers'; they are not in this contract and do not change. The
  `/v1/hq/*` routes below are the node's, which forwards to HQ and audits.
- **Errors** are JSON `{"error": "<plain words>"}` with the status: `400` bad request, `401` no
  usable session, `404` not found, `409` conflicts with what is recorded, `502`/`503` HQ not
  reachable or not set up.

## Areas

| Area | Routes (see the JSON for the full list) |
|---|---|
| Session | `GET /health`; `GET`/`POST /v1/session` (whether sign-in exists; sign in with password and authenticator code); `/v1/session/setup` (claim); `GET /v1/session/current`; `POST /v1/session/sign-out`; `POST /v1/session/sign-out-everywhere`; password, authenticator, checking credential, bootstrap token |
| Work | `GET /v1/hq` (the snapshot), tasks, one item's Live Work, the team map, scenarios and the league, the world record |
| Decisions | `GET /v1/decisions`, `GET`/`POST /v1/decisions/{id}` |
| Runs and evidence | replays of a run and a task, a delivered bundle, Activity, configuration writes |
| Events | `GET /v1/events`, `GET /v1/events/stream` |
| Money and models | credits and caps, the pool's top-up, the Mac's settings, AI, model lists |
| Control | the kill switch and resume, giving work, goals, members, projects, routines, delivery, contracts |
| Home | the web console's home lab, media and smart home: the lab's figures, containers and their logs, the allowlisted jobs, media requests, the house's favorites and catalogue (`GET /v1/terminal/*`), and their one action route `POST /v1/terminal/action`, which Core checks against its own allowlist and runs once per request id. The paths keep their old name; the handheld that first used them is retired |
| Settings | capabilities, tool servers, connections (the Spotify account until Audio's cutover), configuration undo, export and purge |

### The runner in the snapshot

`GET /v1/hq` carries `runner`, so a Console can say when no runner is asking for turns (HQ never
reaches out to one: if every runner stops or loses the network, nothing moves and nothing else says why):

```json
"runner": { "state": "up" | "stale" | "never", "last_poll_secs": 2, "waiting": 0 }
```

- `state`: `up` when a runner made any signed `/v1/runner/*` request (a poll for turns or for stops)
  in the last 90 seconds (the runner asks every 2 s); `stale` when its last request is older;
  `never` when there has been none since HQ started. HQ keeps the time in memory, so it says `never`
  after each restart until a runner next asks, within seconds.
- `last_poll_secs`: whole seconds since that request; `null` when `never`.
- `waiting`: turns HQ has started that no runner has taken yet.
- `runner` is `null` with the demo on (HQ runs no real turns). Treat a missing `runner` (an older HQ)
  as unknown, not as down.
- `runner` is all runners as one: runners share one token, so `up` here means at least one is asking.
  It is unchanged.
- `runners` tells them apart (HQ's snapshot, beside `runner`; `null` with the demo on; absent from an older HQ):

  ```json
  "runners": [ { "name": "runner-a", "state": "up" | "stale", "last_poll_secs": 140 } ]
  ```

  Each runner says its name (`RUNNER_NAME`, sent as `?runner=` on every signed call, so it survives
  the node's team gateway, which passes that one parameter on). HQ keeps ASCII letters, digits, `.`,
  `_` and `-`, at most 32; a call that names none, such as an older runner's, is listed as `unnamed`.
  The list is the quietest first, with the same rule and the same 90 seconds as `runner`, and holds at
  most 16 names (a new name past that drops the one quiet longest). A name from a refused (unsigned)
  call is never listed. `state` is `up` or `stale`; a name is listed only once it has asked.
- The Console shows a loud chip, "Runner `<name>` quiet for ...", when any listed runner is stale, even
  beside one that is up (several: the first and "+N more"), and none when all are up. With no `runners`,
  or one `unnamed` entry, it reads `runner` as before.
- **Limit:** a runner stopped on purpose stays listed as `stale` until HQ restarts (HQ keeps the list in
  memory only, and forgetting one name is not built). A runner is also named only as well as its host
  sets `RUNNER_NAME`: two runners with one name count as one.

## Sessions

Signing in is Core's: `POST /v1/session` with `{"password", "code"}` (an authenticator code)
answers `{"token"}`, throttled and locked against guessing. A console session lasts thirty days
from sign-in and ends after seven days unused (ADR 0125).

- `GET /v1/session/current` → `{"scope", "bootstrap", "issued_ms", "expires_ms",
  "idle_expires_ms"}`: when this session ends. The bootstrap token has no end (`null`s).
- `POST /v1/session/sign-out` ends the session that asks. `POST /v1/session/sign-out-everywhere`
  ends every console session, this one included, and answers `{"ended": n}`. Changing the
  password ends every other console session too.
- An ended or expired session is refused with `401` on its next request, at once: sessions are
  read on every request, never cached.

## Decisions

A decision is an item on the owner's desk, by its stable id. An answer binds the signed-in
person to that exact pending item and a choice the item allows.

- `GET /v1/decisions` → `{"pending": [...], "answered": [...]}`: what waits (HQ's desk and
  *While you were away*, as the snapshot shows them) and the latest answers with their stages.
- `GET /v1/decisions/{id}` → `{"decision": <record or null>, "desk": <the item, with its status>}`.
- `POST /v1/decisions/{id}` with `{"choice", "words"?, "request"?, "channel"?}` answers it.
  `request` is a key the client makes once per press; `channel` names the client
  (`console` by default).

The record:

```json
{
  "id": 42, "kind": "approval", "choice": "approve", "words": "",
  "by": { "person": "owner", "session": "s-1a2b3c4d" }, "channel": "console", "request": "k-…",
  "stages": {
    "requested": { "at": 1730000000000 },
    "accepted":  { "at": 1730000000050 },
    "executed":  { "at": 1730000000400, "ok": true, "said": "…" },
    "verified":  { "at": 1730000000450, "ok": true, "how": "…" }
  }
}
```

- **requested**: Core has the answer and has reserved the item for it.
- **accepted**: HQ recorded the decision on its desk.
- **executed**: what the choice does has run. For most kinds that is the same moment as
  accepted (HQ applies it as it records it); an approval runs the stored tool, a push pushes the
  branch, and each says what came of it, `ok: false` with the reason when it failed.
- **verified**: Core read the desk back afterwards and found the item recorded with that choice
  (and, for a push or an approval, that what ran succeeded). `ok: false` says what did not match.

Choices the item allows, by kind, include (ADR 0149) on a `ready_to_read` card of work in
review: `accept`, `send_back`, `read` and `not_now`. `words` is the optional note.

- `accept` closes the task `done` with closure `why: "accepted_by_owner"` (`by: "owner"`, `at`),
  apart from `met_criteria` and `completed_unverified`, which is never an acceptance.
- `send_back` returns the task to `doing`, posts `words` to its thread and wakes its holder.
- Refusals, `400` with `{"error"}` and nothing recorded, the card still waiting: the task is no
  longer `in_review` (`a task does not go from doing to done`, or `... to doing` for
  `send_back`); the task is gated and its gate has not passed (`the gate has not passed`).
- `send_back` on a task in a project the owner paused is refused `409` with `{"error": "that
  task's project is paused: resume the project first, then send the work back (the card stays
  waiting for you)"}`; nothing is recorded or posted, the task stays `in_review` and the card keeps
  waiting, so the same answer works once the project is resumed. `accept` is allowed there.
  A card whose task is no longer in review (for instance another card of a task already done)
  gets the same `400`; an unknown choice is `400` (`no such decision for this item`).
  A repeated `accept` is not an error: see Idempotent below.

Idempotent: the same choice again (a double press, a retry after a dropped connection) changes
nothing and answers the stored record with `"duplicate": true`, whether the repeat carries the
same request id, a new one or none (`accept` twice is `200`, `duplicate: true`, the first
decision, the task and its closure unchanged, no second post). A different choice for an item
already answered, here or in another client, is `409` with what is recorded. A choice the item
does not allow is refused by HQ (`400`), and nothing is recorded. If HQ cannot be reached the
record stays at **requested** and the answer is `502`; sending the same answer again finishes
it, and never applies it twice.

## Projects and how their work is accepted

`POST /v1/hq/projects` makes a project: `{"goal", "title", "lead", "reason"?, "cap"?, "solo"?,
"acceptance"?}`. Added in v1 (ADR 0149): `acceptance` is `owner` (the default, and what a project
made before it reads as) or `unverified`; any other value is `400` and makes nothing. With a
`solo` member, `owner` leaves each item the member sets ready `in_review` on the owner's desk;
`unverified` has HQ close it itself as `completed_unverified`, "Done, not verified", which is
never shown or counted as accepted. The task's `closure.why` can now be `accepted_by_owner`, and
Live Work's `state` can be `accepted`; a client that does not know them shows the reason's name.

## Events

One stream for Console. The snapshot's `last_event` (`GET /v1/hq`) is the cursor.

- `GET /v1/events?after=<cursor>` → `{"events": [{"id", "at", "kind", "data"}], "cursor",
  "gap", "more"}`, oldest first, at most 500. `more` says to ask again from the new cursor.
  `gap: true` means events after the cursor were pruned: take a new snapshot.
- `GET /v1/events?before=<id>&limit=<n>` → older events, for paging back through Activity.
- `GET /v1/events/stream?after=<cursor>` is server-sent events, signed with the header (a
  `fetch` stream; the query-token exception stays only on the old `/v1/activity/stream`). It
  sends `hello` with the cursor, then each event as `event: hq` with `id:` set to its id, from
  the cursor on, so a reconnect with `Last-Event-ID` (or `after`) catches up exactly; `changed`
  when something outside HQ's record changed (settings, connections); and `gap` when the cursor
  is older than what HQ keeps. The client drops any id at or below the last it applied, so a
  reconnect never applies an event twice.

## What is not in the contract

Sound (`/v1/sound/*`, Audio's own API after its cutover), the Spotify player token (Audio's), the old desk route `/v1/hq/desk/{id}`
(Console answers through `/v1/decisions/{id}`), the query-signed `/v1/activity/stream`, the
workers' relay and gateway, and every HQ route. The built-in console keeps using its routes until
the cutover.
