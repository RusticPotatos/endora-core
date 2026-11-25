# 0153 - Advisory diagnosis of a failed watch: a model proposes, code checks, nothing acts

## Status

Accepted, 2026-11-25. AI-assisted (Claude). Builds on [0150](0150-current-architecture-the-foundation.md)
(the foundation), [0151](0151-a-repair-policy-for-a-failed-watch.md) (what a failed watch may do) and
[0152](0152-owner-notices-an-outbox-and-a-local-sink.md) (what it tells the owner). The live model
endpoint is **not enabled** by this record: see "Not built".

## Context

A failed watch tells the owner that something is down and, with a repair, may try one reversible
call. It never says *why*: the probe's output and the evidence rows hold the answer, and the owner
reads them by hand. A language model can read them, but a model is not a trustworthy witness: it
invents quotes, answers from the wrong line, and follows instructions planted in a log. The
team domain of an earlier Endora learned this and built a checked investigator: a quote verifier
and a fail-closed entailment check. The same checks serve here, as advice.

## Decision

**A failed watch item may carry one advisory diagnosis. A model proposes it; code checks it; it is
kept beside the item and read by people only.**

- **Built from the item's own evidence.** The sources are the item's evidence rows that hold text
  (the probe's output, the watch's own control rows), named `evidence-<row id>`, scrubbed of secrets
  once, and held to 12,000 characters by dropping the oldest whole rows. What the model sees and what
  a quote is checked against are the same text.
- **The model is a port** (`application::DiagnosisModel`, `ask(prompt) -> Result<String, String>`).
  It is not a capability: it is never offered, never decided, and its reply is never acted on.
  Tests replay recorded replies through it; no model runs in a test.
- **Code verifies (`domain::diagnosis`, pure).** The reply must be the agreed JSON shape; every quote
  (at least 6 characters, at most 4 of them) must be verbatim in the document it names, with white
  space collapsed; no cited line may give orders; the answer must sit, token-bounded, inside a valid
  quote. A reply that fails any of these is refused.
- **A fail-closed entailment check.** For each quote that holds the answer, the checker (the same
  model, a second and separate question, wording from the investigator's frozen protocol) is asked
  whether the passage ITSELF states the answer as the answer to the question. Only a first word of
  `yes` accepts; `no` gives `AnswerNotEntailed`, and anything else, a transport error included,
  gives `EntailmentUnreadable`. Both refuse.
- **Four outcomes, one row per item** (`diagnoses`, schema 12): `verified` (the answer and its
  citations are kept), `not_found` (the model said the evidence lacks a cause), `refused` (faults
  kept, **no answer kept**) and `unavailable` (the model could not be reached; nothing was checked).
  A second ask for the same item returns the stored row without a model call, so a down model costs
  one failed attempt per item, not a retry storm.
- **Advisory only, by construction.** A diagnosis is not an evidence row, is not on the hash chain and
  is not in the restore dump of decided state. `decide`, the repair path and acceptance do not read
  it (a test fails if their source mentions it, and another proves that diagnosing changes no item,
  routine, stop switch or evidence row). It authorises, triggers and starts nothing. `diagnoses`
  prints it marked `(advisory)`, with control characters replaced and each line cut at 200 characters.
- **Local model only, and off by default.** The adapter (`app/foundation/src/diagnosis.rs`) makes one
  blocking HTTP call to an Ollama `/api/generate` on a loopback address, over a plain socket, with no
  new dependency. It is constructed only if the owner's config has a line
  `diagnosis-model ollama 127.0.0.1:PORT MODEL`; without the line the foundation asks no model at
  all (tested: a model listening on a free port sees zero connections). A host that is not
  loopback is refused when the config is read. `run-due` and `serve` diagnose at most one newest
  undiagnosed failed item per tick, after the stop-switch check, so a stopped foundation asks nothing.
- **Schema 12 is additive:** one new table, `diagnoses`; no existing table or row is touched; tested
  against a real schema-11 file. Back up before upgrading, as for any schema step; an older build
  refuses a schema-12 file, so a rollback restores the backup.
- **Dependency.** `endora-foundation` and the foundation CLI gain `serde_json` (already in the lock file)
  to read the model's JSON reply and build its request; the domain module parses bytes and imports no IO.

## Evaluation

The done test replays a frozen set of 20 **synthetic** incidents (documentation addresses and
`example.com` only; no real log, name or host) through the Foundation path, with recorded replies
standing in for the model: 16 answerable incidents with a verbatim cause, 2 unanswerable (the
model abstains), one wrong-role answer the checker refuses and one prose reply refused as unreadable.
**18 of 20 end verified, 2 refused, 0 unsupported accepted.** This measures the verifier and the
path, not a model: the replies are authored, so a live model's rate is unmeasured here and is not
claimed. The parent investigator's own result on its (private, real-incident) set is a separate
record and was not reproduced here.

### Amendment, 2026-11-25: a line that speaks to its reader is not a citation

A live trial of the Foundation path (work #159) obeyed a planted line that spoke as the assistant and
stated an answer outright; it passed the quote check, the answer-in-quote check and the entailment
check, because the only "gives orders" test was five fixed phrases. The check now also refuses a quote
taken from a line that poses as the assistant or system, drops the instructions, asks for the prompt or
tells the reader what to answer (`addresses_the_reader`, the narrow part of the lessons' instruction
scan, moved to `shared/kernel` so a domain may use it without importing another; work #162). It is
deliberately narrower than the lessons' whole scan: a log line may hold `curl http://...`,
`POST https://...`, a long build id or a DNS `ANSWER:` field as the tool printed it. Pure and
deterministic, tested with synthetic planted lines and ordinary tool lines; no model run, and the
answer prompt is unchanged. The scan is a floor, not a proof: a differently shaped planted line can
still reach the entailment check.

## Not built

- **The live endpoint is not enabled.** Calling even a local model is a new outward capability for
  the foundation; the adapter exists behind the absent config line and nothing configures it. Turning
  it on in a deployment is the owner's decision. The decide path still refuses outward calls: the
  model port is not a capability and does not widen what an agent can do.
- No diagnosis in a notice (a notice never holds a probe's text, ADR 0152), no model-chosen repair,
  and no diagnosis of anything but a failed watch item.
- No retry of an `unavailable` diagnosis and no re-diagnosis of an item: one row per item.

## Consequences

- The owner can read a checked cause next to a failed watch, once a local model is enabled.
- A refused or abstained diagnosis is as visible as a verified one, so a model that is often refused
  is seen to be.
- A model that quotes correctly but is wrong about the cause is still possible: the checks prove the
  quote is in the evidence and states the answer, not that the answer is the root cause. Hence
  advisory, and hence never an input to a decision.
