use super::{body_json, rpc_post, rpc_post_auth};
use crate::api::{router, AppState};
use tower::ServiceExt;

#[tokio::test]
async fn webrpc_media_relay_methods_use_auth_and_wrapper_shapes() {
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

    let start = app
        .clone()
        .oneshot(rpc_post_auth(
            "StartMediaRelay",
            serde_json::json!({ "roomID": "DEFAULT" }),
            token,
        ))
        .await
        .unwrap();
    assert_eq!(body_json(start).await["mediaRelay"]["status"], "ACTIVE");

    let track = app
        .clone()
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
    let track = body_json(track).await;
    assert_eq!(track["mediaRelay"]["participants"][0]["userID"], user_id);
    assert_eq!(
        track["mediaRelay"]["participants"][0]["tracks"][0]["trackID"],
        "audio-main"
    );
    assert_eq!(
        track["mediaRelay"]["participants"][0]["tracks"][0]["kind"],
        "AUDIO"
    );

    let stop = app
        .oneshot(rpc_post_auth(
            "StopMediaRelay",
            serde_json::json!({ "roomID": "DEFAULT", "userID": user_id }),
            token,
        ))
        .await
        .unwrap();
    assert_eq!(body_json(stop).await["mediaRelay"]["status"], "INACTIVE");
}
