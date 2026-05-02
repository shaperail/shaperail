//! Authenticated subject — role + tenant accessors for custom handlers.

use actix_web::HttpRequest;
use shaperail_core::ShaperailError;

use super::extractor::AuthenticatedUser;

/// The authenticated subject of a request, with role and tenant accessors.
///
/// Use this in custom handlers as the authoritative source of "who is calling
/// and what tenant are they in." Wraps `AuthenticatedUser` with helpers that
/// match the tenant-isolation logic the CRUD path applies automatically.
///
/// # Example
///
/// ```rust,ignore
/// use shaperail_runtime::auth::Subject;
/// use sqlx::QueryBuilder;
///
/// pub async fn regenerate_secret(req: actix_web::HttpRequest, /* state... */) -> actix_web::HttpResponse {
///     let subject = match Subject::from_request(&req) {
///         Ok(s) => s,
///         Err(_) => return actix_web::HttpResponse::Unauthorized().finish(),
///     };
///     let mut q = QueryBuilder::<sqlx::Postgres>::new("UPDATE agents SET mcp_secret_hash = ");
///     q.push_bind(/* new_hash */ "");
///     q.push(" WHERE id = ");
///     q.push_bind(/* agent_id */ uuid::Uuid::nil());
///     subject.scope_to_tenant(&mut q, "org_id").unwrap();
///     // execute q ...
///     actix_web::HttpResponse::Ok().finish()
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Subject {
    pub id: String,
    pub role: String,
    pub tenant_id: Option<String>,
}

impl Subject {
    /// Extracts the subject from an authenticated request. Returns
    /// `Err(ShaperailError::Unauthorized)` if no valid JWT/API key is present.
    pub fn from_request(req: &HttpRequest) -> Result<Self, ShaperailError> {
        let user = super::extractor::try_extract_auth(req).ok_or(ShaperailError::Unauthorized)?;
        Ok(Self::from(&user))
    }

    /// True for the global `super_admin` role, which is exempt from tenant isolation.
    pub fn is_super_admin(&self) -> bool {
        self.role == "super_admin"
    }

    /// The tenant filter to apply to queries.
    ///
    /// - `Ok(None)` for `super_admin` (full visibility).
    /// - `Ok(Some(tenant))` for a normal user with a `tenant_id` claim.
    /// - `Err(Unauthorized)` for a non-`super_admin` subject whose JWT carries
    ///   no `tenant_id` claim — that is a configuration error, not a silent
    ///   "no filter" pass.
    pub fn tenant_filter(&self) -> Result<Option<&str>, ShaperailError> {
        if self.is_super_admin() {
            return Ok(None);
        }
        match self.tenant_id.as_deref() {
            Some(t) if !t.is_empty() => Ok(Some(t)),
            _ => Err(ShaperailError::Unauthorized),
        }
    }

    /// Asserts that a record's tenant column matches this subject's tenant.
    ///
    /// - `Ok(())` for `super_admin` (no check applied).
    /// - `Ok(())` for a normal user whose tenant matches `record_tenant_id`.
    /// - `Err(Forbidden)` for a normal user whose tenant does NOT match.
    /// - `Err(Unauthorized)` for a normal user with no `tenant_id` claim.
    pub fn assert_tenant_match(&self, record_tenant_id: &str) -> Result<(), ShaperailError> {
        match self.tenant_filter()? {
            None => Ok(()),
            Some(t) if t == record_tenant_id => Ok(()),
            Some(_) => Err(ShaperailError::Forbidden),
        }
    }

    /// Appends a tenant filter to a sqlx `QueryBuilder` for tenant-scoped queries.
    /// No-op for `super_admin`.
    ///
    /// Pushes `" AND <column> = "` followed by a bound `tenant_id`. Caller is
    /// responsible for the surrounding query shape.
    pub fn scope_to_tenant<'q>(
        &self,
        builder: &mut sqlx::QueryBuilder<'q, sqlx::Postgres>,
        column: &str,
    ) -> Result<(), ShaperailError> {
        let Some(tenant) = self.tenant_filter()? else {
            return Ok(());
        };
        builder.push(" AND ");
        builder.push(column);
        builder.push(" = ");
        builder.push_bind(tenant.to_string());
        Ok(())
    }
}

impl From<&AuthenticatedUser> for Subject {
    fn from(user: &AuthenticatedUser) -> Self {
        Self {
            id: user.id.clone(),
            role: user.role.clone(),
            tenant_id: user.tenant_id.clone(),
        }
    }
}

impl From<AuthenticatedUser> for Subject {
    fn from(user: AuthenticatedUser) -> Self {
        Self::from(&user)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn super_admin() -> Subject {
        Subject {
            id: "u1".into(),
            role: "super_admin".into(),
            tenant_id: None,
        }
    }

    fn member(tenant: &str) -> Subject {
        Subject {
            id: "u2".into(),
            role: "member".into(),
            tenant_id: Some(tenant.into()),
        }
    }

    fn member_no_tenant() -> Subject {
        Subject {
            id: "u3".into(),
            role: "member".into(),
            tenant_id: None,
        }
    }

    #[test]
    fn super_admin_tenant_filter_is_none() {
        assert!(super_admin().tenant_filter().unwrap().is_none());
    }

    #[test]
    fn member_tenant_filter_is_their_tenant() {
        let s = member("org-1");
        assert_eq!(s.tenant_filter().unwrap(), Some("org-1"));
    }

    #[test]
    fn member_without_tenant_is_unauthorized() {
        let s = member_no_tenant();
        assert!(matches!(
            s.tenant_filter(),
            Err(ShaperailError::Unauthorized)
        ));
    }

    #[test]
    fn assert_tenant_match_super_admin_skips_check() {
        super_admin().assert_tenant_match("any").unwrap();
    }

    #[test]
    fn assert_tenant_match_mismatch_is_forbidden() {
        let s = member("org-1");
        assert!(matches!(
            s.assert_tenant_match("org-2"),
            Err(ShaperailError::Forbidden)
        ));
    }

    #[test]
    fn assert_tenant_match_match_ok() {
        member("org-1").assert_tenant_match("org-1").unwrap();
    }

    #[test]
    fn scope_to_tenant_super_admin_is_noop() {
        let mut b = sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT 1");
        super_admin().scope_to_tenant(&mut b, "org_id").unwrap();
        assert_eq!(b.sql(), "SELECT 1");
    }

    #[test]
    fn scope_to_tenant_member_appends_filter() {
        let mut b = sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT 1");
        member("org-1").scope_to_tenant(&mut b, "org_id").unwrap();
        // sqlx renders the bind placeholder as $1, $2, ...; assert structure.
        let sql = b.sql();
        assert!(sql.starts_with("SELECT 1 AND "));
        assert!(sql.contains("org_id = $1"));
    }

    // ── Additional coverage ────────────────────────────────────────────────

    #[test]
    fn is_super_admin_true() {
        assert!(super_admin().is_super_admin());
    }

    #[test]
    fn is_super_admin_false_for_member() {
        assert!(!member("org-1").is_super_admin());
    }

    #[test]
    fn is_super_admin_false_for_admin_role() {
        let s = Subject {
            id: "u1".into(),
            role: "admin".into(),
            tenant_id: Some("org-1".into()),
        };
        assert!(!s.is_super_admin(), "'admin' role is not 'super_admin'");
    }

    #[test]
    fn empty_string_tenant_id_treated_as_missing() {
        // tenant_id = Some("") is semantically "not set"
        let s = Subject {
            id: "u1".into(),
            role: "member".into(),
            tenant_id: Some(String::new()),
        };
        assert!(
            matches!(s.tenant_filter(), Err(ShaperailError::Unauthorized)),
            "empty-string tenant_id must be treated as Unauthorized"
        );
    }

    #[test]
    fn scope_to_tenant_error_propagates_when_no_tenant() {
        let mut b = sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT 1");
        let result = member_no_tenant().scope_to_tenant(&mut b, "org_id");
        assert!(
            matches!(result, Err(ShaperailError::Unauthorized)),
            "scope_to_tenant must propagate Unauthorized for tenant-less member"
        );
        // Query must be unmodified on error
        assert_eq!(b.sql(), "SELECT 1");
    }

    #[test]
    fn from_authenticated_user_conversion() {
        use super::super::extractor::AuthenticatedUser;

        let user = AuthenticatedUser {
            id: "u-99".into(),
            role: "viewer".into(),
            tenant_id: Some("org-42".into()),
        };
        let subject = Subject::from(user);
        assert_eq!(subject.id, "u-99");
        assert_eq!(subject.role, "viewer");
        assert_eq!(subject.tenant_id.as_deref(), Some("org-42"));
    }

    #[test]
    fn subject_clone() {
        let s = member("org-1");
        let c = s.clone();
        assert_eq!(c.id, s.id);
        assert_eq!(c.role, s.role);
        assert_eq!(c.tenant_id, s.tenant_id);
    }

    #[test]
    fn assert_tenant_match_empty_string_record_tenant() {
        // Even if record_tenant is empty, it must match if subject tenant is also empty
        let s = Subject {
            id: "u1".into(),
            role: "super_admin".into(),
            tenant_id: None,
        };
        // super_admin skips the check for any value
        s.assert_tenant_match("").unwrap();
    }
}
