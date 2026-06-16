use super::{body_json, offer_sdp, rpc_post, rpc_post_auth};
use crate::api::{router, AppState};
use axum::http::StatusCode;
use lyre_core::{
    MediaTrackKind, RegisterMediaTrackRequest, RoomId, StartMediaRelayRequest, UserId,
};
use tower::ServiceExt;

#[tokio::test]
async fn webrpc_server_media_methods_return_wrapper_shapes() {
    let app = router(AppState::default());
    let join = app
        .clone()
        .oneshot(rpc_post(
            "JoinRoom",
            serde_json::json!({ "roomID": "DEFAULT", "nickname": "Ada" }),
        ))
        .await
        .unwrap();
    let join = body_json(join).await;
    let user_id = join["user"]["id"].as_str().unwrap();
    let token = join["accessToken"].as_str().unwrap();

    app.clone()
        .oneshot(rpc_post_auth(
            "StartMediaRelay",
            serde_json::json!({ "roomID": "DEFAULT" }),
            token,
        ))
        .await
        .unwrap();
    app.clone()
        .oneshot(rpc_post_auth(
            "RegisterMediaTrack",
            serde_json::json!({
                "roomID": "DEFAULT",
                "userID": user_id,
                "trackID": "audio-main",
                "kind": "AUDIO",
            }),
            token,
        ))
        .await
        .unwrap();

    let answer = app
        .clone()
        .oneshot(rpc_post_auth(
            "AnswerServerMediaOffer",
            serde_json::json!({
                "roomID": "DEFAULT",
                "userID": user_id,
                "audioTrackID": "audio-main",
                "sdp": offer_sdp().await,
            }),
            token,
        ))
        .await
        .unwrap();
    assert_eq!(answer.status(), StatusCode::OK);
    let answer = body_json(answer).await;
    assert_eq!(answer["answer"]["roomID"], "DEFAULT");
    assert_eq!(answer["answer"]["userID"], user_id);
    assert_eq!(answer["answer"]["audioTrackID"], "audio-main");
    assert_eq!(answer["answer"]["state"], "NEGOTIATING");
    assert!(answer["answer"]["sdp"].as_str().unwrap().starts_with("v=0"));

    let candidate_text = "candidate:1 1 UDP 2130706431 192.168.1.100 54321 typ host";
    let accepted = app
        .clone()
        .oneshot(rpc_post_auth(
            "AddServerMediaIceCandidate",
            serde_json::json!({
                "roomID": "DEFAULT",
                "userID": user_id,
                "candidate": candidate_text,
                "sdpMid": "0",
                "sdpMLineIndex": 0,
                "usernameFragment": null,
            }),
            token,
        ))
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::OK);
    let accepted = body_json(accepted).await;
    assert_eq!(accepted["accepted"]["roomID"], "DEFAULT");
    assert_eq!(accepted["accepted"]["userID"], user_id);
    assert_eq!(accepted["accepted"]["candidate"], candidate_text);

    let mut candidates = serde_json::Value::Null;
    for _ in 0..128 {
        let response = app
            .clone()
            .oneshot(rpc_post_auth(
                "GetServerMediaIceCandidates",
                serde_json::json!({ "roomID": "DEFAULT", "userID": user_id }),
                token,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        candidates = body_json(response).await;
        if candidates["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|candidate| {
                candidate["roomID"] == "DEFAULT"
                    && candidate["userID"] == user_id
                    && candidate["candidate"]
                        .as_str()
                        .unwrap()
                        .starts_with("candidate:")
            })
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(candidates["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|candidate| { candidate["roomID"] == "DEFAULT" && candidate["userID"] == user_id }));

    let closed = app
        .oneshot(rpc_post_auth(
            "CloseServerMediaSession",
            serde_json::json!({ "roomID": "DEFAULT", "userID": user_id }),
            token,
        ))
        .await
        .unwrap();
    assert_eq!(closed.status(), StatusCode::OK);
    let closed = body_json(closed).await;
    assert_eq!(closed["closed"]["mediaRelay"]["status"], "ACTIVE");
    assert_eq!(closed["closed"]["session"]["state"], "CLOSED");
}

#[tokio::test]
async fn webrpc_server_media_methods_reject_missing_bearer_token() {
    let app = router(AppState::default());
    let response = app
        .oneshot(rpc_post(
            "AnswerServerMediaOffer",
            serde_json::json!({
                "roomID": "DEFAULT",
                "userID": "user_01",
                "audioTrackID": "audio-main",
                "sdp": offer_sdp().await,
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(response).await;
    assert_eq!(body["name"], "WebrpcEndpoint");
    assert_eq!(body["message"], "room access token is invalid");
}

#[tokio::test]
async fn webrpc_server_media_errors_do_not_echo_sdp_or_ice() {
    let state = AppState::default();
    let app = router(state.clone());
    let join = app
        .clone()
        .oneshot(rpc_post(
            "JoinRoom",
            serde_json::json!({ "roomID": "DEFAULT", "nickname": "Ada" }),
        ))
        .await
        .unwrap();
    let join = body_json(join).await;
    let user_id = join["user"]["id"].as_str().unwrap();
    let token = join["accessToken"].as_str().unwrap();
    state
        .media_relays
        .start(RoomId::default_room(), StartMediaRelayRequest::default());
    state
        .media_relays
        .register_track(
            RoomId::default_room(),
            RegisterMediaTrackRequest {
                user_id: UserId::from_external(user_id),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    let secret_sdp = "not sdp with secret-token candidate:secret";

    let response = app
        .oneshot(rpc_post_auth(
            "AnswerServerMediaOffer",
            serde_json::json!({
                "roomID": "DEFAULT",
                "userID": user_id,
                "audioTrackID": "audio-main",
                "sdp": secret_sdp,
            }),
            token,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response).await;
    let body_text = body.to_string();
    assert_eq!(body["name"], "WebrpcEndpoint");
    assert_eq!(body["message"], "server media negotiation failed");
    assert!(body.get("error").is_none());
    assert!(!body_text.contains(secret_sdp));
    assert!(!body_text.contains("secret-token"));
    assert!(!body_text.contains("candidate:secret"));
}
