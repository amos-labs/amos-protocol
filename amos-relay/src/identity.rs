//! Authenticated identities and explicit authority. Request JSON never selects
//! the caller's identity. Service wallet delegation is provisioned out of band.

use axum::http::{Method, StatusCode};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Principal {
    Agent {
        id: Uuid,
        wallet: String,
    },
    Harness {
        id: String,
    },
    Service {
        id: Uuid,
        name: String,
        scopes: Vec<String>,
        wallets: Vec<String>,
    },
}

pub async fn whoami(
    axum::Extension(principal): axum::Extension<Principal>,
) -> axum::Json<Principal> {
    axum::Json(principal)
}

impl Principal {
    pub fn require_agent(&self, requested: Uuid) -> Result<(), StatusCode> {
        match self {
            Self::Agent { id, .. } if *id == requested => Ok(()),
            _ => Err(StatusCode::FORBIDDEN),
        }
    }

    pub fn require_harness(&self, requested: &str) -> Result<(), StatusCode> {
        match self {
            Self::Harness { id } if id == requested => Ok(()),
            _ => Err(StatusCode::FORBIDDEN),
        }
    }

    pub fn require_scope(&self, scope: &str) -> Result<Uuid, StatusCode> {
        match self {
            Self::Service { id, scopes, .. } if scopes.iter().any(|s| s == scope) => Ok(*id),
            _ => Err(StatusCode::FORBIDDEN),
        }
    }

    pub fn require_wallet(&self, requested: &str) -> Result<(), StatusCode> {
        match self {
            Self::Agent { wallet, .. } if wallet == requested => Ok(()),
            Self::Service { wallets, .. } if wallets.iter().any(|w| w == requested) => Ok(()),
            _ => Err(StatusCode::FORBIDDEN),
        }
    }

    pub fn require_reviewer(&self, wallet: &str) -> Result<(), StatusCode> {
        if matches!(self, Self::Service { .. }) {
            self.require_scope("bounties:review")?;
        }
        self.require_wallet(wallet)
    }

    pub fn agent_wallet(&self, id: Uuid, supplied: Option<&str>) -> Result<String, StatusCode> {
        self.require_agent(id)?;
        match self {
            Self::Agent { wallet, .. } if supplied.is_none_or(|w| w == wallet) => {
                Ok(wallet.clone())
            }
            _ => Err(StatusCode::FORBIDDEN),
        }
    }

    pub fn audit_id(&self) -> String {
        match self {
            Self::Agent { id, .. } => format!("agent:{id}"),
            Self::Harness { id } => format!("harness:{id}"),
            Self::Service { id, .. } => format!("service:{id}"),
        }
    }
}

/// Strict formats allow UUIDs and old short API keys to fail before a DB query.
pub fn credential_kind(token: &str) -> Option<&str> {
    let (kind, entropy) = token.split_once('_')?;
    (matches!(kind, "agent" | "harness" | "service")
        && entropy.len() == 64
        && entropy
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)))
    .then_some(kind)
}

pub fn public_route(method: &Method, path: &str) -> bool {
    let path = path.trim_end_matches('/');
    if *method == Method::POST {
        return matches!(
            path,
            "/api/v1/agents/challenge"
                | "/api/v1/agents/register"
                | "/api/v1/harnesses/connect"
                | "/api/v1/webhooks/github"
        );
    }
    *method == Method::GET
        && (path == "/health"
            || path == "/api/v1/pool/today"
            || path == "/api/v1/agents"
            || path.starts_with("/api/v1/agents/")
            || path == "/api/v1/bounties"
            || path.starts_with("/api/v1/bounties/"))
}

pub async fn authenticate(db: &PgPool, token: &str) -> Result<Principal, StatusCode> {
    let kind = credential_kind(token).ok_or(StatusCode::UNAUTHORIZED)?;
    let hash = crate::middleware::hash_api_key(token);
    match kind {
        "agent" => {
            let row: Option<(Uuid, String)> = sqlx::query_as(
                "SELECT id, wallet_address FROM relay_agents WHERE api_key_hash = $1 AND wallet_verified AND status IN ('active', 'inactive')")
                .bind(hash).fetch_optional(db).await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            row.map(|(id, wallet)| Principal::Agent { id, wallet })
                .ok_or(StatusCode::UNAUTHORIZED)
        }
        "harness" => {
            let row: Option<(String,)> = sqlx::query_as(
                "SELECT harness_id FROM relay_harnesses WHERE api_key_hash = $1 AND status IN ('active', 'inactive')")
                .bind(hash).fetch_optional(db).await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            row.map(|(id,)| Principal::Harness { id })
                .ok_or(StatusCode::UNAUTHORIZED)
        }
        "service" => {
            let row: Option<(Uuid, String, Vec<String>, Vec<String>)> = sqlx::query_as(
                "SELECT id, name, scopes, bound_wallets FROM relay_service_credentials WHERE api_key_hash = $1 AND active")
                .bind(hash).fetch_optional(db).await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            row.map(|(id, name, scopes, wallets)| Principal::Service {
                id,
                name,
                scopes,
                wallets,
            })
            .ok_or(StatusCode::UNAUTHORIZED)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

/// Old claim-time wallet strings are not payment authority. The recipient must
/// still be the wallet proved by that particular claimed agent.
pub async fn verified_claim_wallet(
    db: &PgPool,
    agent: Option<Uuid>,
    claimed: Option<&str>,
) -> Result<String, StatusCode> {
    let agent = agent.ok_or(StatusCode::FORBIDDEN)?;
    let wallet: Option<String> = sqlx::query_scalar(
        "SELECT wallet_address FROM relay_agents WHERE id = $1 AND wallet_verified AND status = 'active'")
        .bind(agent).fetch_optional(db).await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let wallet = wallet.ok_or(StatusCode::FORBIDDEN)?;
    if claimed.is_some_and(|w| w != wallet) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(wallet)
}

pub async fn settlement_reviewer_wallet(db: &PgPool, bounty: Uuid) -> Result<String, StatusCode> {
    sqlx::query_scalar("SELECT a.wallet_address FROM relay_bounties b JOIN relay_agents a ON a.wallet_address=b.reviewer_wallet WHERE b.id=$1 AND b.approved_by_principal IS NOT NULL AND a.wallet_verified AND a.status='active' AND a.trust_level>=5 AND a.council_member")
        .bind(bounty).fetch_optional(db).await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::FORBIDDEN)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifiers_and_weak_or_unknown_credentials_are_rejected() {
        for token in [
            Uuid::new_v4().to_string(),
            "agent_short".into(),
            format!("agent_{}", "g".repeat(64)),
            format!("other_{}", "a".repeat(64)),
        ] {
            assert_eq!(credential_kind(&token), None);
        }
        for kind in ["agent", "harness", "service"] {
            assert_eq!(
                credential_kind(&crate::middleware::generate_api_key(kind)),
                Some(kind)
            );
        }
    }
    #[test]
    fn agent_cannot_impersonate_wallet_or_other_agent_or_service() {
        let id = Uuid::new_v4();
        let p = Principal::Agent {
            id,
            wallet: "alice".into(),
        };
        assert_eq!(p.agent_wallet(id, None).unwrap(), "alice");
        assert!(p.agent_wallet(id, Some("bob")).is_err());
        assert!(p.agent_wallet(Uuid::new_v4(), Some("alice")).is_err());
        assert!(p.require_reviewer("bob").is_err());
        assert!(p.require_scope("reputation:write").is_err());
    }
    #[test]
    fn service_requires_both_review_scope_and_explicit_wallet() {
        let mut p = Principal::Service {
            id: Uuid::new_v4(),
            name: "oracle".into(),
            scopes: vec!["bounties:review".into()],
            wallets: vec!["qa".into()],
        };
        assert!(p.require_reviewer("qa").is_ok());
        assert!(p.require_reviewer("victim").is_err());
        assert!(p.require_scope("reputation:write").is_err());
        if let Principal::Service { scopes, .. } = &mut p {
            scopes.clear();
        }
        assert!(p.require_reviewer("qa").is_err());
    }
    #[test]
    fn harness_cannot_mutate_other_harness_or_economic_identity() {
        let p = Principal::Harness { id: "mine".into() };
        assert!(p.require_harness("mine").is_ok());
        assert!(p.require_harness("other").is_err());
        assert!(p.require_reviewer("qa").is_err());
        assert!(p.require_scope("bounties:create").is_err());
    }
    #[test]
    fn public_routes_are_method_specific_and_registration_is_exact() {
        assert!(public_route(&Method::POST, "/api/v1/agents/register"));
        assert!(public_route(&Method::GET, "/api/v1/agents/123"));
        for path in [
            "/api/v1/agents/register/anything",
            "/api/v1/pool/today",
            "/api/v1/bounties/123/approve",
            "/api/v1/webhooks/anything",
        ] {
            assert!(!public_route(&Method::POST, path));
        }
    }
}
