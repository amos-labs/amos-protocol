//! Isolated PostgreSQL/Redis regression. Run with run-identity-integration.sh;
//! never points at a configured Relay database or chain RPC.
use amos_relay::{middleware::hash_api_key, server::build_http_router, RelayState};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use solana_sdk::signature::{Keypair, Signer};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

async fn call(
    app: &Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn register(app: &Router, signer: &Keypair) -> (Value, Value) {
    let wallet = signer.pubkey().to_string();
    let (status, challenge) = call(
        app,
        "POST",
        "/api/v1/agents/challenge",
        None,
        json!({"wallet_address":wallet}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let request = json!({"name":"synthetic-agent","display_name":"Synthetic Agent","endpoint_url":"https://example.test/agent","capabilities":[],"wallet_address":wallet,
        "wallet_proof":{"challenge_id":challenge["challenge_id"],"signature":signer.sign_message(challenge["message"].as_str().unwrap().as_bytes()).to_string()}});
    let (status, agent) = call(
        app,
        "POST",
        "/api/v1/agents/register",
        None,
        request.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{agent}");
    (agent, request)
}

async fn submitted_system_bounty(
    app: &Router,
    poster: &str,
    creator: &str,
    worker_id: &str,
    worker_key: &str,
) -> Uuid {
    let (status, created) = call(app, "POST", "/api/v1/bounties", Some(creator), json!({
        "title":"Synthetic system task", "description":"Synthetic local verification",
        "reward_tokens":100,"deadline":(chrono::Utc::now()+chrono::Duration::days(1)).to_rfc3339(),
        "required_capabilities":[],"poster_wallet":poster,"category":"content"
    })).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        call(
            app,
            "POST",
            &format!("/api/v1/bounties/{id}/claim"),
            Some(worker_key),
            json!({"agent_id":worker_id,"harness_id":""})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            app,
            "POST",
            &format!("/api/v1/bounties/{id}/submit"),
            Some(worker_key),
            json!({"agent_id":worker_id,"result":{"version":"A"}})
        )
        .await
        .0,
        StatusCode::OK
    );
    id
}

#[tokio::test]
#[ignore = "requires isolated services; run tests/run-identity-integration.sh"]
async fn authenticated_registration_and_http_mutation_boundaries() {
    let url = std::env::var("PROTOCOL_TEST_DATABASE_URL").expect("isolated test DB required");
    let redis_url = std::env::var("PROTOCOL_TEST_REDIS_URL").expect("isolated Redis required");
    assert!(url.starts_with("postgresql://127.0.0.1:") && url.ends_with("/protocol_identity_test"));
    assert!(redis_url.starts_with("redis://127.0.0.1:"));
    let db = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    let redis = redis::aio::ConnectionManager::new(redis::Client::open(redis_url).unwrap())
        .await
        .unwrap();
    let config = serde_json::from_value(json!({"database":{"url":url}})).unwrap();
    let app = build_http_router(RelayState {
        db: db.clone(),
        redis,
        config: Arc::new(config),
        solana: None,
    });

    let signer = Keypair::new();
    let (agent, used_request) = register(&app, &signer).await;
    let id = agent["id"].as_str().unwrap();
    let key = agent["api_key"].as_str().unwrap();
    assert_eq!(key.len(), 70);
    let stored: (String, bool) =
        sqlx::query_as("SELECT api_key_hash,wallet_verified FROM relay_agents WHERE id=$1")
            .bind(Uuid::parse_str(id).unwrap())
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(stored, (hash_api_key(key), true));
    let (_, directory) = call(
        &app,
        "GET",
        &format!("/api/v1/agents/{id}"),
        None,
        Value::Null,
    )
    .await;
    assert!(directory.get("api_key").is_none() && directory.get("api_key_hash").is_none());
    assert_eq!(
        call(&app, "POST", "/api/v1/agents/register", None, used_request)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/v1/agents/{id}/heartbeat"),
            Some(id),
            json!({})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/v1/agents/{id}/heartbeat"),
            Some(key),
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/v1/agents/{}/heartbeat", Uuid::new_v4()),
            Some(key),
            json!({})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    // A different signer and expired challenges cannot enroll a claimed wallet.
    let (_, challenge) = call(
        &app,
        "POST",
        "/api/v1/agents/challenge",
        None,
        json!({"wallet_address":signer.pubkey().to_string()}),
    )
    .await;
    let mut forged = json!({"name":"a","display_name":"a","endpoint_url":"https://example.test","capabilities":[],"wallet_address":signer.pubkey().to_string(),"wallet_proof":{"challenge_id":challenge["challenge_id"],"signature":Keypair::new().sign_message(challenge["message"].as_str().unwrap().as_bytes()).to_string()}});
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/v1/agents/register",
            None,
            forged.clone()
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    forged["wallet_proof"]["signature"] = signer
        .sign_message(challenge["message"].as_str().unwrap().as_bytes())
        .to_string()
        .into();
    sqlx::query(
        "UPDATE relay_identity_challenges SET expires_at=now()-interval '1 second' WHERE id=$1",
    )
    .bind(Uuid::parse_str(challenge["challenge_id"].as_str().unwrap()).unwrap())
    .execute(&db)
    .await
    .unwrap();
    assert_eq!(
        call(&app, "POST", "/api/v1/agents/register", None, forged)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );

    // Registration rotation requires a fresh wallet signature and revokes old keys.
    let (rotated, _) = register(&app, &signer).await;
    assert_eq!(rotated["id"], agent["id"]);
    assert_eq!(
        call(&app, "GET", "/api/v1/identity/me", Some(key), Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let key = rotated["api_key"].as_str().unwrap();
    let bounty = Uuid::new_v4();
    assert_eq!(call(&app, "POST", &format!("/api/v1/bounties/{bounty}/claim"), Some(key), json!({"agent_id":Uuid::new_v4(),"harness_id":"","wallet_address":signer.pubkey().to_string()})).await.0, StatusCode::FORBIDDEN);
    for (route, body) in [
        (
            "verify",
            json!({"verifier_wallet":Keypair::new().pubkey().to_string(),"evidence":{"test":"passed"}}),
        ),
        (
            "approve",
            json!({"reviewer_wallet":Keypair::new().pubkey().to_string()}),
        ),
        (
            "reject",
            json!({"reviewer_wallet":Keypair::new().pubkey().to_string(),"reason":"test"}),
        ),
        (
            "request_revision",
            json!({"reviewer_wallet":Keypair::new().pubkey().to_string(),"feedback":"test"}),
        ),
        (
            "pushback",
            json!({"reviewer_wallet":Keypair::new().pubkey().to_string(),"reason":"test"}),
        ),
        ("settle", json!({})),
        (
            "record-merge",
            json!({"merge_commit_sha":"a".repeat(40),"merged_by":"somebody-else"}),
        ),
    ] {
        assert_eq!(
            call(
                &app,
                "POST",
                &format!("/api/v1/bounties/{bounty}/{route}"),
                Some(key),
                body
            )
            .await
            .0,
            StatusCode::FORBIDDEN,
            "{route}"
        );
    }
    assert_eq!(call(&app, "POST", "/api/v1/reputation/report", Some(key), json!({"harness_id":"h","agent_id":id,"task_id":"t","outcome":"completed","quality_score":100})).await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/v1/escalations",
            Some(key),
            json!({"decision_id":Uuid::new_v4(),"path":"intake","reason":"test"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    let harness_body = json!({"harness_id":"synthetic-harness","name":"Synthetic","version":"1","endpoint_url":"https://example.test/harness","api_key":"ignored-weak-key"});
    let (status, harness) = call(
        &app,
        "POST",
        "/api/v1/harnesses/connect",
        None,
        harness_body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let harness_key = harness["api_key"].as_str().unwrap();
    assert_eq!(harness_key.len(), 72);
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/v1/harnesses/connect",
            None,
            harness_body.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/v1/harnesses/connect",
            Some(key),
            harness_body.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, reconnect) = call(
        &app,
        "POST",
        "/api/v1/harnesses/connect",
        Some(harness_key),
        harness_body,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(reconnect.get("api_key").is_none());

    let service_key = amos_relay::middleware::generate_api_key("service");
    sqlx::query("INSERT INTO relay_service_credentials(id,name,api_key_hash,scopes,bound_wallets) VALUES ($1,'synthetic-oracle',$2,$3,$4)")
        .bind(Uuid::new_v4()).bind(hash_api_key(&service_key)).bind(vec!["bounties:review"]).bind(vec![signer.pubkey().to_string()]).execute(&db).await.unwrap();
    let (status, who) = call(
        &app,
        "GET",
        "/api/v1/identity/me",
        Some(&service_key),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(who["kind"], "service");
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/v1/bounties/{bounty}/settle"),
            Some(&service_key),
            json!({})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/v1/bounties/{bounty}/approve"),
            Some(&service_key),
            json!({"reviewer_wallet":Keypair::new().pubkey().to_string()})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    // Scoped reporting exercises the real UUID harness FK/VARCHAR outcome
    // schema; historical unauthenticated reports cannot boost current trust.
    let reporter_key = amos_relay::middleware::generate_api_key("service");
    let reporter_id = Uuid::new_v4();
    sqlx::query("INSERT INTO relay_service_credentials(id,name,api_key_hash,scopes) VALUES ($1,'synthetic-reporter',$2,$3)")
        .bind(reporter_id).bind(hash_api_key(&reporter_key)).bind(vec!["reputation:write"]).execute(&db).await.unwrap();
    let harness_id = harness["harness_id"].as_str().unwrap();
    let report_body = json!({"harness_id":harness_id,"agent_id":id,"task_id":"synthetic-task","outcome":"completed","quality_score":87});
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/v1/reputation/report",
            Some(&service_key),
            report_body.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, report) = call(
        &app,
        "POST",
        "/api/v1/reputation/report",
        Some(&reporter_key),
        report_body,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{report}");
    assert_eq!(report["harness_id"], harness_id);
    assert_eq!(report["outcome"], "completed");
    let row: (Uuid,Uuid,String) = sqlx::query_as("SELECT authenticated_reporter,harness_id,task_id FROM relay_reputation_reports WHERE id=$1")
        .bind(Uuid::parse_str(report["id"].as_str().unwrap()).unwrap()).fetch_one(&db).await.unwrap();
    assert_eq!(row.0, reporter_id);
    let stored_harness_id: Uuid =
        sqlx::query_scalar("SELECT id FROM relay_harnesses WHERE harness_id=$1")
            .bind(harness_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(row.1, stored_harness_id);
    assert_eq!(row.2, "synthetic-task");
    sqlx::query("INSERT INTO relay_reputation_reports(agent_id,harness_id,outcome,quality_score) VALUES ($1,$2,'completed',100)")
        .bind(Uuid::parse_str(id).unwrap()).bind(row.1).execute(&db).await.unwrap();
    let (status, reputation) = call(
        &app,
        "GET",
        &format!("/api/v1/reputation/{id}"),
        Some(key),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reputation["total_tasks"], 1);
    assert_eq!(reputation["quality_score"], 87.0);
    let (status, _) = call(&app,"POST","/api/v1/reputation/report",Some(&reporter_key),json!({"harness_id":harness_id,"agent_id":id,"task_id":"synthetic-failure","outcome":"failed"})).await;
    assert_eq!(status, StatusCode::CREATED);
    let stats: (i64,i64,f64,f64) = sqlx::query_as("SELECT total_bounties_completed,total_bounties_failed,completion_rate,avg_quality_score FROM relay_agents WHERE id=$1")
        .bind(Uuid::parse_str(id).unwrap()).fetch_one(&db).await.unwrap();
    assert_eq!(stats, (1, 1, 0.5, 87.0));

    // Authenticated system approval is not a commercial fee event.
    let reviewer_signer = Keypair::new();
    let (reviewer, _) = register(&app, &reviewer_signer).await;
    let reviewer_key = reviewer["api_key"].as_str().unwrap();
    let reviewer_wallet = reviewer_signer.pubkey().to_string();
    sqlx::query("UPDATE relay_agents SET trust_level=5,council_member=true WHERE id=$1")
        .bind(Uuid::parse_str(reviewer["id"].as_str().unwrap()).unwrap())
        .execute(&db)
        .await
        .unwrap();
    let creator_key = amos_relay::middleware::generate_api_key("service");
    let poster_wallet = Keypair::new().pubkey().to_string();
    sqlx::query("INSERT INTO relay_service_credentials(id,name,api_key_hash,scopes,bound_wallets) VALUES ($1,'synthetic-creator',$2,$3,$4)")
        .bind(Uuid::new_v4()).bind(hash_api_key(&creator_key)).bind(vec!["bounties:create"]).bind(vec![&poster_wallet]).execute(&db).await.unwrap();
    let approved = submitted_system_bounty(&app, &poster_wallet, &creator_key, id, key).await;
    let verify_body =
        json!({"verifier_wallet":reviewer_wallet,"evidence":{"synthetic_check":true}});
    let approve_body = json!({"reviewer_wallet":reviewer_wallet,"quality_score":90});
    // Legacy timestamps without principal provenance cannot authorize payout.
    sqlx::query("UPDATE relay_bounties SET verified_at=now(),verified_by_wallet=$2 WHERE id=$1")
        .bind(approved)
        .bind(&reviewer_wallet)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/v1/bounties/{approved}/approve"),
            Some(reviewer_key),
            approve_body.clone()
        )
        .await
        .0,
        StatusCode::PRECONDITION_REQUIRED
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/v1/bounties/{approved}/verify"),
            Some(reviewer_key),
            verify_body.clone()
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, approved_body) = call(
        &app,
        "POST",
        &format!("/api/v1/bounties/{approved}/approve"),
        Some(reviewer_key),
        approve_body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{approved_body}");
    let provenance: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT verified_by_principal,approved_by_principal FROM relay_bounties WHERE id=$1",
    )
    .bind(approved)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        provenance.0,
        Some(format!("agent:{}", reviewer["id"].as_str().unwrap()))
    );
    assert_eq!(provenance.1, provenance.0);
    let fees: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM protocol_fee_ledger WHERE bounty_id=$1")
            .bind(approved)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(fees, 0);

    // A NULL historical wallet does not hide that the actual worker is also
    // the reviewer. Both verify and approve resolve the claimed agent.
    let self_review = submitted_system_bounty(&app, &poster_wallet, &creator_key, id, key).await;
    sqlx::query(
        "UPDATE relay_bounties SET claimed_by_agent_id=$2,claimed_by_wallet=NULL WHERE id=$1",
    )
    .bind(self_review)
    .bind(Uuid::parse_str(reviewer["id"].as_str().unwrap()).unwrap())
    .execute(&db)
    .await
    .unwrap();
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/v1/bounties/{self_review}/verify"),
            Some(reviewer_key),
            verify_body.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE relay_bounties SET verified_at=now(),verified_by_principal='service:synthetic-prior-verifier' WHERE id=$1")
        .bind(self_review).execute(&db).await.unwrap();
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/v1/bounties/{self_review}/approve"),
            Some(reviewer_key),
            approve_body.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    // Hold a real row lock after the HTTP reader can see submission A. Once
    // its UPDATE blocks, publish B and release: approval must return conflict.
    let raced = submitted_system_bounty(&app, &poster_wallet, &creator_key, id, key).await;
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/v1/bounties/{raced}/verify"),
            Some(reviewer_key),
            verify_body.clone()
        )
        .await
        .0,
        StatusCode::OK
    );
    let mut concurrent = db.begin().await.unwrap();
    sqlx::query("SELECT id FROM relay_bounties WHERE id=$1 FOR UPDATE")
        .bind(raced)
        .fetch_one(&mut *concurrent)
        .await
        .unwrap();
    let approval_app = app.clone();
    let approval_key = reviewer_key.to_string();
    let approval_body = approve_body.clone();
    let pending = tokio::spawn(async move {
        call(
            &approval_app,
            "POST",
            &format!("/api/v1/bounties/{raced}/approve"),
            Some(&approval_key),
            approval_body,
        )
        .await
    });
    let mut blocked = false;
    for _ in 0..500 {
        blocked = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'UPDATE relay_bounties SET %approved_by_principal%')")
            .fetch_one(&db).await.unwrap();
        if blocked {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        blocked,
        "HTTP approval must reach the blocked compare-and-set update"
    );
    // Keep verification populated here to isolate the exact-version guard.
    sqlx::query("UPDATE relay_bounties SET result='{\"version\":\"B\"}',updated_at=updated_at+interval '1 second' WHERE id=$1")
        .bind(raced).execute(&mut *concurrent).await.unwrap();
    concurrent.commit().await.unwrap();
    assert_eq!(pending.await.unwrap().0, StatusCode::CONFLICT);
    let state: String = sqlx::query_scalar("SELECT status FROM relay_bounties WHERE id=$1")
        .bind(raced)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(state, "submitted");
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/v1/bounties/{raced}/request_revision"),
            Some(reviewer_key),
            json!({"reviewer_wallet":reviewer_wallet,"feedback":"Revise synthetic submission"})
        )
        .await
        .0,
        StatusCode::OK
    );
    let cleared: (bool,bool,bool) = sqlx::query_as("SELECT verified_at IS NULL,verified_by_principal IS NULL,proof_receipt IS NULL FROM relay_bounties WHERE id=$1").bind(raced).fetch_one(&db).await.unwrap();
    assert_eq!(cleared, (true, true, true));

    sqlx::query("UPDATE relay_service_credentials SET active=false")
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/v1/identity/me",
            Some(&service_key),
            Value::Null
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}
