//! X (formerly Twitter) live chat capability gate (slice 6 of the In-App Livestream Comments
//! plan: `2026-06-06 - Videorc In-App Livestream Comments Plan`).
//!
//! X is a supported Videorc streaming platform, but there is no verified, self-serve native
//! X live-comments/chat API. This module is capability-reporting ONLY: it never scrapes,
//! embeds, or fakes X comments. X reports `unsupported` ("pending API access") until a
//! verified native path exists. When one does, satisfy the evidence checklist below, flip
//! `X_NATIVE_COMMENTS_AVAILABLE`, and add the real connector behind this same gate.

use serde::Serialize;

/// Whether a verified, approved native X live-comments path exists AND is implemented here.
///
/// Stays `false` until [`X_COMMENTS_EVIDENCE_CHECKLIST`] is satisfied. The plan forbids
/// shipping X comments — or marking the whole comments feature done — without that evidence,
/// so this is the single switch a future X connector slice must justify flipping.
pub const X_NATIVE_COMMENTS_AVAILABLE: bool = false;

/// The evidence that must exist before X live comments may be built (plan "Required Evidence
/// Before Build"). Surfaced to the UI/diagnostics so the gate is auditable rather than a
/// silent assumption.
pub const X_COMMENTS_EVIDENCE_CHECKLIST: &[&str] = &[
    "Official public X API documentation for live video comments or chat.",
    "Approved partner or API access with documented endpoints and allowed app behavior.",
    "An approved native provider path testable with real X livestream comments.",
];

/// The X chat provider message, given whether an X account is connected. With an account the
/// platform is streaming but comments are blocked on API access; without one it is the
/// generic native-access notice (covers manual RTMP too).
pub fn x_chat_message(has_x_account: bool) -> &'static str {
    if X_NATIVE_COMMENTS_AVAILABLE {
        "X live comments are ready."
    } else if has_x_account {
        "X comments pending API access."
    } else {
        "X comments require native X API access."
    }
}

/// An auditable readiness report for X live comments: the capability reporter + evidence gate
/// surfaced over the `liveChat.xCommentsReadiness` command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XChatReadiness {
    /// True only when a verified native path exists and is implemented (currently never).
    pub available: bool,
    pub message: String,
    pub evidence_checklist: Vec<String>,
}

pub fn x_chat_readiness(has_x_account: bool) -> XChatReadiness {
    XChatReadiness {
        available: X_NATIVE_COMMENTS_AVAILABLE,
        message: x_chat_message(has_x_account).to_string(),
        evidence_checklist: X_COMMENTS_EVIDENCE_CHECKLIST
            .iter()
            .map(|item| (*item).to_string())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x_native_comments_gate_stays_closed() {
        // Guard / smoke placeholder: this MUST remain false until a verified native X
        // comments path is approved. Flipping it without the evidence checklist would ship an
        // unsupported (or faked) feature — exactly what the plan prohibits.
        assert!(
            !x_chat_readiness(true).available,
            "X comments must stay gated until a verified native path exists"
        );
        assert_eq!(X_COMMENTS_EVIDENCE_CHECKLIST.len(), 3);
    }

    #[test]
    fn message_reflects_account_and_gate() {
        assert_eq!(
            x_chat_message(false),
            "X comments require native X API access."
        );
        assert_eq!(x_chat_message(true), "X comments pending API access.");
    }

    #[test]
    fn readiness_is_unsupported_with_full_evidence_checklist() {
        let readiness = x_chat_readiness(true);
        assert!(!readiness.available);
        assert_eq!(readiness.message, "X comments pending API access.");
        assert_eq!(readiness.evidence_checklist.len(), 3);
    }
}
