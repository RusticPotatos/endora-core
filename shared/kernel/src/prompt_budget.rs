//! A turn's prompt budget (ADR 0142, amended 2026-10-30): what a turn may read in all, in bytes,
//! before the worker ends it and HQ continues the item in a fresh turn. One pair of figures that
//! two sides must say alike: the team domain picks one by the member's brain and HQ judges the
//! turn by it, and the worker ends the turn at it.

/// What a house turn (the NAS's model, the Mac, DeepSeek) may read in all, in bytes: about
/// 60,000 tokens at four bytes a token. The Mac reads about 100 tokens a second, so that is
/// about ten minutes of reading in a turn; measured, a ten-call task turn read about 74,600
/// tokens with old results stubbed and work turns 110,000 to 200,000 (ADR 0142, amended
/// 2026-10-30).
pub const HOUSE_PROMPT_BUDGET: usize = 240_000;

/// What a Claude turn may read in all, in bytes: about 120,000 tokens, the measured ten-call
/// task turn with every result whole (about 119,500), so a turn of that length still ends on
/// its own and only the long ones (110,000 to 200,000 and more) go on fresh.
pub const CLAUDE_PROMPT_BUDGET: usize = 480_000;

/// The NAS model's context window, in tokens: Ollama runs it with `OLLAMA_CONTEXT_LENGTH=16384`
/// and one slot (ADR 0085; a probe measured the slot at 16,384). A prompt past it is
/// truncated, which ADR 0085 recorded as breaking turns.
pub const NAS_CONTEXT_TOKENS: usize = 16_384;

/// What the NAS model keeps of its window for its reply, in tokens: its tool calls are short
/// and its thinking is bounded by the turn's rounds, so 2,048 leaves room for both.
pub const NAS_REPLY_TOKENS: usize = 2_048;

/// The fewest bytes a token of the NAS model's prompt is taken to be. A probe measured 3.45
/// on a real turn's tool schemas, prompt and briefing; three leaves a margin for denser text
/// (code, paths, JSON escapes).
pub const NAS_BYTES_A_TOKEN: usize = 3;

/// What one call to the NAS model may send, in bytes: the window less the reply's
/// room, at the fewest bytes a token. The NAS counts a turn's conversation once, so the worker
/// holds every call to this by itself, the gist's side call too; a call that would pass it ends
/// the turn instead, and HQ goes on in a fresh one. 43,008 bytes, about 12,500 tokens as
/// measured. Only the first call is made without the check, and a real first call (tool
/// schemas, prompt and briefing) measured about 11,700 bytes.
pub const NAS_PROMPT_BUDGET: usize = (NAS_CONTEXT_TOKENS - NAS_REPLY_TOKENS) * NAS_BYTES_A_TOKEN;

/// About how many bytes of a prompt make a token.
pub const BYTES_A_TOKEN: usize = 4;
