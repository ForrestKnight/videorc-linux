use crate::protocol::{AccountStatus, VideorcAccountSnapshot};

// Real web auth + token storage are not wired yet (there is no videorc.com
// token-exchange endpoint to call). Two stand-ins keep the flow exercisable until
// it lands, neither of which can affect a production build:
//   * VIDEORC_MOCK_ACCOUNT=username — a dev/debug-only env override (mock_account_from_env).
//   * complete_mock_sign_in — a dev/debug-only resolver for the deep-link token.
// Release builds ignore both and stay signed-out, so production can never be
// spoofed into a signed-in state. The in-memory session override (held in
// AppState) lets the deep-link sign in and Sign out clear it; persistent secure
// token storage replaces all of this once the endpoint exists.
pub const MOCK_ACCOUNT_ENV_VAR: &str = "VIDEORC_MOCK_ACCOUNT";

// Resolve the effective account: an explicit in-memory session override wins
// (set by the deep-link sign-in or by Sign out); otherwise fall back to the
// dev-only env mock, otherwise signed-out.
pub fn current_account(
    session_override: Option<&VideorcAccountSnapshot>,
) -> VideorcAccountSnapshot {
    match session_override {
        Some(snapshot) => snapshot.clone(),
        None => mock_account_from_env(
            std::env::var(MOCK_ACCOUNT_ENV_VAR).ok().as_deref(),
            cfg!(debug_assertions),
        ),
    }
}

fn mock_account_from_env(value: Option<&str>, dev_build: bool) -> VideorcAccountSnapshot {
    match value.map(str::trim).filter(|username| !username.is_empty()) {
        Some(username) if dev_build => signed_in_mock(username),
        _ => signed_out_account(),
    }
}

// Resolve a session token delivered by the account deep-link into an account.
// Real resolution exchanges the token with videorc.com (not built); dev builds
// accept a mock token (the token string is used as the username) so the sign-in
// flow is exercisable. Release builds resolve nothing → caller stays signed-out.
pub fn complete_mock_sign_in(token: &str, dev_build: bool) -> Option<VideorcAccountSnapshot> {
    let token = token.trim();
    if dev_build && !token.is_empty() {
        return Some(signed_in_mock(token));
    }
    None
}

fn signed_in_mock(username: &str) -> VideorcAccountSnapshot {
    VideorcAccountSnapshot {
        status: AccountStatus::SignedIn,
        username: Some(username.to_string()),
        display_name: None,
        email: None,
    }
}

pub fn signed_out_account() -> VideorcAccountSnapshot {
    VideorcAccountSnapshot {
        status: AccountStatus::SignedOut,
        username: None,
        display_name: None,
        email: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_account_is_signed_out_without_an_override_or_mock_env() {
        // No override and no VIDEORC_MOCK_ACCOUNT in the test env -> signed-out.
        let account = current_account(None);
        assert_eq!(account.status, AccountStatus::SignedOut);
        assert!(account.username.is_none());
    }

    #[test]
    fn an_in_memory_session_override_wins_over_the_env_mock() {
        let signed_in = signed_in_mock("orc_dev");
        assert_eq!(current_account(Some(&signed_in)), signed_in);
        // An explicit signed-out override (from Sign out) also wins.
        assert_eq!(
            current_account(Some(&signed_out_account())).status,
            AccountStatus::SignedOut
        );
    }

    #[test]
    fn mock_account_signs_in_only_in_dev_builds_with_the_env_set() {
        let mocked = mock_account_from_env(Some("orc_dev"), true);
        assert_eq!(mocked.status, AccountStatus::SignedIn);
        assert_eq!(mocked.username.as_deref(), Some("orc_dev"));
    }

    #[test]
    fn release_builds_ignore_the_mock_env_and_stay_signed_out() {
        assert_eq!(
            mock_account_from_env(Some("orc_dev"), false),
            signed_out_account()
        );
        assert_eq!(mock_account_from_env(None, true), signed_out_account());
        assert_eq!(
            mock_account_from_env(Some("   "), true),
            signed_out_account()
        );
    }

    #[test]
    fn deep_link_token_resolves_to_an_account_only_in_dev_builds() {
        let resolved = complete_mock_sign_in("orc_dev", true).unwrap();
        assert_eq!(resolved.status, AccountStatus::SignedIn);
        assert_eq!(resolved.username.as_deref(), Some("orc_dev"));
        // Release builds have no resolver, and blank tokens never resolve.
        assert!(complete_mock_sign_in("orc_dev", false).is_none());
        assert!(complete_mock_sign_in("   ", true).is_none());
    }

    #[test]
    fn signed_out_account_omits_optional_fields_and_round_trips() {
        let json = serde_json::to_string(&signed_out_account()).unwrap();
        assert_eq!(json, r#"{"status":"signed-out"}"#);
        let restored: VideorcAccountSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, signed_out_account());
    }

    #[test]
    fn signed_in_account_serializes_identity_fields_in_camel_case() {
        let account = VideorcAccountSnapshot {
            status: AccountStatus::SignedIn,
            username: Some("orc_dev".to_string()),
            display_name: Some("Orc Dev".to_string()),
            email: Some("orc@videorc.com".to_string()),
        };
        let json = serde_json::to_string(&account).unwrap();
        assert!(json.contains("\"status\":\"signed-in\""));
        assert!(json.contains("\"displayName\":\"Orc Dev\""));
        let restored: VideorcAccountSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, account);
    }
}
