use shaperail_core::{AuthRule, ShaperailError};

use super::extractor::AuthenticatedUser;

/// Enforces RBAC rules from an endpoint's `auth` specification.
///
/// - `AuthRule::Public` — always allowed, `user` may be `None`
/// - `AuthRule::Roles(roles)` — user must be authenticated with a matching role.
///   If the roles list contains "owner", ownership check is deferred to `check_owner`.
/// - `AuthRule::Owner` — user must own the resource (checked via `check_owner`)
///
/// Returns `Ok(())` if access is granted, `Err(Unauthorized)` or `Err(Forbidden)` otherwise.
pub fn enforce(
    auth_rule: Option<&AuthRule>,
    user: Option<&AuthenticatedUser>,
) -> Result<(), ShaperailError> {
    let rule = match auth_rule {
        None => return Ok(()), // No auth rule = public
        Some(rule) => rule,
    };

    match rule {
        AuthRule::Public => Ok(()),
        AuthRule::Owner => {
            // Owner check requires authentication; actual ownership verified separately
            if user.is_none() {
                return Err(ShaperailError::Unauthorized);
            }
            Ok(())
        }
        AuthRule::Roles(roles) => {
            let authenticated = user.ok_or(ShaperailError::Unauthorized)?;

            // Check if user's role is in the allowed roles list
            // "owner" in roles is handled by the caller via check_owner
            if roles.iter().any(|r| r == &authenticated.role) {
                return Ok(());
            }

            // If "owner" is in the roles list, we don't reject here —
            // the caller will do the ownership check
            if roles.iter().any(|r| r == "owner") {
                return Ok(());
            }

            Err(ShaperailError::Forbidden)
        }
    }
}

/// Checks if the authenticated user owns the given resource record.
///
/// Looks for a `created_by` field in the record and compares it to `user.id`.
/// Returns `Ok(())` if the user is the owner, `Err(Forbidden)` otherwise.
pub fn check_owner(
    user: &AuthenticatedUser,
    record: &serde_json::Value,
) -> Result<(), ShaperailError> {
    let created_by = record.get("created_by").and_then(|v| v.as_str());

    match created_by {
        Some(owner_id) if owner_id == user.id => Ok(()),
        _ => Err(ShaperailError::Forbidden),
    }
}

/// Returns true if the auth rule requires an ownership check for this user.
///
/// This is true when:
/// - The rule is `AuthRule::Owner`
/// - The rule is `AuthRule::Roles` with "owner" in the list, AND the user's
///   role is not directly in the roles list (so they need ownership to pass)
pub fn needs_owner_check(auth_rule: Option<&AuthRule>, user: Option<&AuthenticatedUser>) -> bool {
    let rule = match auth_rule {
        Some(r) => r,
        None => return false,
    };

    match rule {
        AuthRule::Owner => true,
        AuthRule::Roles(roles) => {
            if !roles.iter().any(|r| r == "owner") {
                return false;
            }
            // If user's role is already in the list, no ownership check needed
            match user {
                Some(u) => !roles.iter().any(|r| r != "owner" && r == &u.role),
                None => false,
            }
        }
        AuthRule::Public => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admin_user() -> AuthenticatedUser {
        AuthenticatedUser {
            id: "user-1".to_string(),
            role: "admin".to_string(),
        }
    }

    fn member_user() -> AuthenticatedUser {
        AuthenticatedUser {
            id: "user-2".to_string(),
            role: "member".to_string(),
        }
    }

    fn viewer_user() -> AuthenticatedUser {
        AuthenticatedUser {
            id: "user-3".to_string(),
            role: "viewer".to_string(),
        }
    }

    #[test]
    fn public_allows_anyone() {
        assert!(enforce(Some(&AuthRule::Public), None).is_ok());
        assert!(enforce(Some(&AuthRule::Public), Some(&admin_user())).is_ok());
    }

    #[test]
    fn none_auth_allows_anyone() {
        assert!(enforce(None, None).is_ok());
    }

    #[test]
    fn roles_requires_auth() {
        let rule = AuthRule::Roles(vec!["admin".to_string()]);
        let result = enforce(Some(&rule), None);
        assert!(matches!(result, Err(ShaperailError::Unauthorized)));
    }

    #[test]
    fn roles_allows_matching_role() {
        let rule = AuthRule::Roles(vec!["admin".to_string(), "member".to_string()]);
        assert!(enforce(Some(&rule), Some(&admin_user())).is_ok());
        assert!(enforce(Some(&rule), Some(&member_user())).is_ok());
    }

    #[test]
    fn roles_denies_non_matching_role() {
        let rule = AuthRule::Roles(vec!["admin".to_string()]);
        let result = enforce(Some(&rule), Some(&viewer_user()));
        assert!(matches!(result, Err(ShaperailError::Forbidden)));
    }

    #[test]
    fn owner_requires_auth() {
        let result = enforce(Some(&AuthRule::Owner), None);
        assert!(matches!(result, Err(ShaperailError::Unauthorized)));
    }

    #[test]
    fn owner_allows_authenticated() {
        // enforce only checks that user is present; actual ownership check is separate
        assert!(enforce(Some(&AuthRule::Owner), Some(&admin_user())).is_ok());
    }

    #[test]
    fn check_owner_matches() {
        let user = AuthenticatedUser {
            id: "user-1".to_string(),
            role: "member".to_string(),
        };
        let record = serde_json::json!({"id": "rec-1", "created_by": "user-1"});
        assert!(check_owner(&user, &record).is_ok());
    }

    #[test]
    fn check_owner_rejects_other() {
        let user = AuthenticatedUser {
            id: "user-1".to_string(),
            role: "member".to_string(),
        };
        let record = serde_json::json!({"id": "rec-1", "created_by": "user-2"});
        assert!(matches!(
            check_owner(&user, &record),
            Err(ShaperailError::Forbidden)
        ));
    }

    #[test]
    fn check_owner_rejects_missing_field() {
        let user = admin_user();
        let record = serde_json::json!({"id": "rec-1"});
        assert!(matches!(
            check_owner(&user, &record),
            Err(ShaperailError::Forbidden)
        ));
    }

    #[test]
    fn roles_with_owner_allows_through() {
        // When roles include "owner" and user role isn't in the list,
        // enforce still passes — caller does ownership check
        let rule = AuthRule::Roles(vec!["admin".to_string(), "owner".to_string()]);
        assert!(enforce(Some(&rule), Some(&viewer_user())).is_ok());
    }

    #[test]
    fn needs_owner_check_for_owner_rule() {
        assert!(needs_owner_check(
            Some(&AuthRule::Owner),
            Some(&admin_user())
        ));
    }

    #[test]
    fn needs_owner_check_roles_with_owner() {
        let rule = AuthRule::Roles(vec!["admin".to_string(), "owner".to_string()]);
        // Admin is in the roles, so no owner check needed
        assert!(!needs_owner_check(Some(&rule), Some(&admin_user())));
        // Viewer is NOT in the roles, needs owner check
        assert!(needs_owner_check(Some(&rule), Some(&viewer_user())));
    }

    #[test]
    fn needs_owner_check_false_for_public() {
        assert!(!needs_owner_check(Some(&AuthRule::Public), None));
    }

    #[test]
    fn needs_owner_check_false_for_roles_without_owner() {
        let rule = AuthRule::Roles(vec!["admin".to_string()]);
        assert!(!needs_owner_check(Some(&rule), Some(&admin_user())));
    }
}
