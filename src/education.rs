//! Progressive within-session teaching.
//!
//! The tool's static docs deliberately describe only the minimal contract:
//! anchor the complete text of the code being edited, old_string style. The
//! prefix-anchor shorthand is taught here instead, as `===TIP===` sections
//! appended to responses — emitted only after a session has demonstrated
//! success with the basics, and only while the behavior the tip teaches
//! remains unadopted. Every session starts untaught: there is no
//! cross-session proficiency tracking, so short sessions that never need the
//! shorthand never pay for learning it.
//!
//! Tips terminate *behaviorally*, not by shown-once bookkeeping: a harness may
//! filter tool results before the model sees them (efference triages every
//! result through a fork), so emission is no proof of teaching. Instead each
//! tip keeps firing, on a cadence, while the inputs still exhibit the
//! pre-tip style — and stops the moment the style changes or a hard emission
//! cap is reached.
//!
//! The duplicate-content warning below is *not* a gated tip: since edits
//! persist immediately, re-sending applied content is a correctness hazard
//! (the earlier placement is already in the file), so it fires every time.

use serde::{Deserialize, Serialize};

/// How many successful edits before the first prefix-shorthand tip, and the
/// minimum number of further successes between repeat emissions.
const PREFIX_TIP_CADENCE: u32 = 5;
/// Hard cap on prefix-shorthand tip emissions per session.
const PREFIX_TIP_MAX_EMISSIONS: u32 = 3;

/// Per-session record of demonstrated proficiency and tips already emitted.
/// Lives in the private session store and is reset by `set_working_directory`,
/// the closest available proxy for "a new session began".
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct EducationState {
    successful_edits: u32,
    prefix_tip_emissions: u32,
    prefix_tip_last_emitted_at: u32,
}

impl EducationState {
    /// Whether the prefix-shorthand tip would be emitted for a successful edit
    /// applied right now with a multi-line anchor. Checked *before* recording
    /// the success so the caller can decide whether to pay for verifying a
    /// shorthand suggestion.
    pub fn prefix_tip_due(&self) -> bool {
        let count_after_this_edit = self.successful_edits + 1;
        self.prefix_tip_emissions < PREFIX_TIP_MAX_EMISSIONS
            && count_after_this_edit >= self.prefix_tip_last_emitted_at + PREFIX_TIP_CADENCE
    }

    pub fn record_success(&mut self) {
        self.successful_edits += 1;
    }

    pub fn record_prefix_tip_emitted(&mut self) {
        self.prefix_tip_emissions += 1;
        self.prefix_tip_last_emitted_at = self.successful_edits;
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

/// Warning appended when an insert re-applied the exact content of the
/// previous (already-persisted) edit at a different location: the file now
/// contains both placements, which is only sometimes what was meant.
pub fn duplicate_insert_warning() -> &'static str {
    "===WARNING===\nThis content is identical to the previous edit applied to this file, and \
that earlier placement is still present — the file now contains both. If the earlier placement \
was a mistake, remove it with a `replace` edit (omit `content` to delete) anchored on the \
misplaced copy. Next time, `retarget_edit` moves the previous edit to a corrected anchor in \
one step."
}
