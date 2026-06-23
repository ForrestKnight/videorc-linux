use crate::protocol::{AccountStatus, VideorcAccountSnapshot};

// Real web auth + token storage are not wired yet (there is no videorc.com
// token-exchange endpoint to call). To exercise the signed-in UI before that
// lands, dev/debug builds honor VIDEORC_MOCK_ACCOUNT=username and report a mock
// signed-in account. Release builds ignore it and always report signed-out, so a
// production build can never be spoofed into a signed-in state. Real token
// storage (resolving a session token via the endpoint) replaces this mock.
pub const MOCK_ACCOUNT_ENV_VAR: &str = "VIDEORC_MOCK_ACCOUNT";

pub fn current_account() -> VideorcAccountSnapshot {
    mock_account_from_env(
        std::env::var(MOCK_ACCOUNT_ENV_VAR).ok().as_deref(),
        cfg!(debug_assertions),
    )
}

fn mock_account_from_env(value: Option<&str>, dev_build: bool) -> VideorcAccountSnapshot {
    match value.map(str::trim).filter(|username| !username.is_empty()) {
        Some(username) if dev_build => signed_in_mock(username),
        _ => signed_out_account(),
    }
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
    fn current_account_is_signed_out_without_a_mock_or_stored_session() {
        // No VIDEORC_MOCK_ACCOUNT in the test env, so this is signed-out.
        let account = current_account();
        assert_eq!(account.status, AccountStatus::SignedOut);
        assert!(account.username.is_none());
        assert!(account.email.is_none());
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
    }

    #[test]
    fn dev_builds_without_the_env_or_with_a_blank_value_stay_signed_out() {
        assert_eq!(mock_account_from_env(None, true), signed_out_account());
        assert_eq!(
            mock_account_from_env(Some("   "), true),
            signed_out_account()
        );
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
