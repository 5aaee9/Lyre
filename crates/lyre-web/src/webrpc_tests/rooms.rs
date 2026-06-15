use super::{body_json, rpc_post, rpc_post_auth};
use crate::api::{router, AppState};
use axum::http::StatusCode;
use tower::ServiceExt;

#[tokio::test]
async fn webrpc_join_get_and_leave_use_generated_client_shape() {
    let app = router(AppState::default());

    let join = app
        .clone()
        .oneshot(rpc_post(
            "JoinRoom",
            serde_json::json!({
                "roomID": "DEFAULT",
                "nickname": "Ada",
                "noise": {
                    "provider": "OFF",
                    "intensity": 0.5,
                    "voiceActivityThreshold": 0.35,
                },
            }),
        ))
        .await
        .unwrap();
    assert_eq!(join.status(), StatusCode::OK);
    let join_body = body_json(join).await;
    let user_id = join_body["user"]["id"].as_str().unwrap();
    let access_token = join_body["accessToken"].as_str().unwrap();
    assert_eq!(join_body["room"]["roomID"], "DEFAULT");
    assert!(join_body["user"]["joinedAt"].is_string());
    assert_eq!(join_body["user"]["noise"]["provider"], "OFF");

    let room = app
        .clone()
        .oneshot(rpc_post(
            "GetRoom",
            serde_json::json!({ "roomID": "DEFAULT" }),
        ))
        .await
        .unwrap();
    assert_eq!(
        body_json(room).await["room"]["users"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let missing_auth = app
        .clone()
        .oneshot(rpc_post(
            "LeaveRoom",
            serde_json::json!({ "roomID": "DEFAULT", "userID": user_id }),
        ))
        .await
        .unwrap();
    assert_eq!(missing_auth.status(), StatusCode::UNAUTHORIZED);
    let error = body_json(missing_auth).await;
    assert_eq!(error["name"], "WebrpcEndpoint");
    assert!(error.get("error").is_none());

    let leave = app
        .oneshot(rpc_post_auth(
            "LeaveRoom",
            serde_json::json!({ "roomID": "DEFAULT", "userID": user_id }),
            access_token,
        ))
        .await
        .unwrap();
    assert_eq!(leave.status(), StatusCode::OK);
    assert_eq!(
        body_json(leave).await["room"]["users"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn webrpc_public_discovery_methods_return_wrapper_shapes() {
    let app = router(AppState::default());

    let providers = app
        .clone()
        .oneshot(rpc_post("GetNoiseProviders", serde_json::json!({})))
        .await
        .unwrap();
    let providers = body_json(providers).await;
    assert_eq!(providers["providers"][0]["provider"], "OFF");

    let ice_servers = app
        .clone()
        .oneshot(rpc_post("GetIceServers", serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(
        body_json(ice_servers).await["iceServers"][0]["urls"][0],
        "stun:stun.l.google.com:19302"
    );

    let topology = app
        .clone()
        .oneshot(rpc_post("GetMediaTopology", serde_json::json!({})))
        .await
        .unwrap();
    let topology = body_json(topology).await;
    assert_eq!(topology["topology"]["mode"], "MEDIA_RELAY");
    assert_eq!(
        topology["topology"]["serverNoiseCancellingRequires"],
        "MEDIA_RELAY"
    );

    let relay = app
        .oneshot(rpc_post(
            "GetMediaRelay",
            serde_json::json!({ "roomID": "DEFAULT" }),
        ))
        .await
        .unwrap();
    let relay = body_json(relay).await;
    assert_eq!(relay["mediaRelay"]["roomID"], "DEFAULT");
    assert_eq!(relay["mediaRelay"]["status"], "INACTIVE");
}
