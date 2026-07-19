//! Progressive within-session teaching.
//!
//! The tool's static docs deliberately describe only the minimal contract:
//! anchor the complete text of the code being edited, old_string style. The
//! shorthands (prefix anchors, `retarget_edit`) are taught here instead, as
//! `===TIP===` sections appended to responses — emitted only after a session
//! has demonstrated success with the basics, and only while the behavior the
//! tip teaches remains unadopted. Every session starts untaught: there is no
//! cross-session proficiency tracking, so short sessions that never need the
//! shorthands never pay for learning them.
//!
//! Tips terminate *behaviorally*, not by shown-once bookkeeping: a harness may
//! filter tool results before the model sees them (efference triages every
//! result through a fork), so emission is no proof of teaching. Instead each
//! tip keeps firing, on a cadence, while the inputs still exhibit the
//! pre-tip style — and stops the moment the style changes or a hard emission
//! cap is reached.

use serde::{Deserialize, Serialize};

/// How many successful stagings before the first prefix-shorthand tip, and the
/// minimum number of further successes between repeat emissions.
const PREFIX_TIP_CADENCE: u32 = 5;
/// Hard cap on prefix-shorthand tip emissions per session.
const PREFIX_TIP_MAX_EMISSIONS: u32 = 3;
/// Hard cap on retarget tip emissions per session.
const RETARGET_TIP_MAX_EMISSIONS: u32 = 2;
/// Content shorter than this isn't worth a `retarget_edit` round trip, so
/// re-sending it doesn't warrant a tip.
const RETARGET_TIP_MIN_CONTENT_LEN: usize = 80;

/// Per-session record of demonstrated proficiency and tips already emitted.
/// Lives in the private session store and is reset by `set_working_directory`,
/// the closest available proxy for "a new session began".
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct EducationState {
    successful_edits: u32,
    prefix_tip_emissions: u32,
    prefix_tip_last_emitted_at: u32,
    retarget_tip_emissions: u32,
}

impl EducationState {
    /// Whether the prefix-shorthand tip would be emitted for a successful edit
    /// staged right now with a multi-line anchor. Checked *before* recording
    /// the success so the caller can decide whether to pay for verifying a
    /// shorthand suggestion.
    pub fn prefix_tip_due(&self) -> bool {
        let count_after_this_edit = self.successful_edits + 1;
        self.prefix_tip_emissions < PREFIX_TIP_MAX_EMISSIONS
            && count_after_this_edit >= self.prefix_tip_last_emitted_at + PREFIX_TIP_CADENCE
    }

    /// Whether re-sending staged content of the given length warrants the
    /// `retarget_edit` tip.
    pub fn retarget_tip_due(&self, content_len: usize) -> bool {
        self.retarget_tip_emissions < RETARGET_TIP_MAX_EMISSIONS
            && content_len >= RETARGET_TIP_MIN_CONTENT_LEN
    }

    pub fn record_success(&mut self) {
        self.successful_edits += 1;
    }

    pub fn record_prefix_tip_emitted(&mut self) {
        self.prefix_tip_emissions += 1;
        self.prefix_tip_last_emitted_at = self.successful_edits;
    }

    pub fn record_retarget_tip_emitted(&mut self) {
        self.retarget_tip_emissions += 1;
    }
}

/// The prefix-shorthand lesson, grounded in the edit that just succeeded.
/// `shorthand` must already be *verified* — re-running the edit with it must
/// have produced the identical diff — so the tip never teaches an anchor that
/// would have behaved differently.
pub fn prefix_tip(shorthand: &str) -> String {
    format!(
        "===TIP===\nThis edit would have produced the identical result with a shorter anchor:\n\n  \
anchor: {shorthand:?}\n\nAn anchor only needs to be unique in the file and to cover the start of \
the code it targets — the rest of the target's text can be omitted. This is especially useful for \
deletes: a one-line anchor with `content` omitted removes the whole item it begins."
    )
}

/// The retarget lesson, emitted when staged content was re-sent verbatim with
/// different targeting.
pub fn retarget_tip() -> String {
    "===TIP===\nThis edit's content was identical to the already-staged edit. When only the \
targeting needs to change, `retarget_edit` re-aims the staged edit at a new anchor without \
resending the content."
        .to_string()
}
