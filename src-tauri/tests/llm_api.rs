//! Integration tests for the LLM panel's HTTP half (#99), against wiremock.
//!
//! The discovery half that needs SSH and `tailscale status` is unit-tested in
//! `llm.rs` as pure candidate-list functions; what is left to check here is the
//! part that talks to a real socket: what counts as a hit, which candidate wins
//! when several answer, and that a stream decodes and terminates.

use std::sync::Mutex;

use proxmox_desktop_lib::llm::{first_endpoint, stream_chat, ChatChunk, ChatMessage};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const MODELS_BODY: &str = r#"{"object":"list","data":[{"id":"qwen3-30b-a3b","object":"model"}]}"#;

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap()
}

/// A server that answers `/v1/models` like llama-server does.
async fn serving() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(MODELS_BODY, "application/json"))
        .mount(&server)
        .await;
    server
}

/// A server that is up but serves no LLM -- the common case for a guest.
async fn not_serving() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn probe_hit_reports_the_model_list() {
    let server = serving().await;
    let found = first_endpoint(&client(), vec![server.uri()]).await.unwrap();
    assert_eq!(found.models, vec!["qwen3-30b-a3b"]);
    assert_eq!(found.base_url, server.uri());
    assert!(!found.manual);
}

#[tokio::test]
async fn probe_miss_is_none_not_an_error() {
    let server = not_serving().await;
    assert!(first_endpoint(&client(), vec![server.uri()])
        .await
        .is_none());
}

/// The driving case in miniature: the guest's own address does not answer, a
/// later candidate does, and that is the one reported.
#[tokio::test]
async fn a_later_candidate_wins_when_the_first_misses() {
    let dead = not_serving().await;
    let live = serving().await;
    let found = first_endpoint(&client(), vec![dead.uri(), live.uri()])
        .await
        .unwrap();
    assert_eq!(found.base_url, live.uri());
}

/// Both the tailnet address and the node's DNAT answer on the real box, so the
/// winner has to be decided by list order, not by which socket replied first.
/// This is what `buffered` buys over `buffer_unordered`.
#[tokio::test]
async fn priority_order_decides_when_several_answer() {
    let first = serving().await;
    let second = serving().await;
    let found = first_endpoint(&client(), vec![first.uri(), second.uri()])
        .await
        .unwrap();
    assert_eq!(found.base_url, first.uri());
}

/// An unreachable address must not sink the whole probe.
#[tokio::test]
async fn an_unreachable_candidate_is_skipped() {
    let live = serving().await;
    // TEST-NET-1 (RFC 5737): guaranteed not to route anywhere.
    let found = first_endpoint(&client(), vec!["http://192.0.2.1:8080".into(), live.uri()])
        .await
        .unwrap();
    assert_eq!(found.base_url, live.uri());
}

fn sse(events: &[&str]) -> String {
    events
        .iter()
        .map(|e| format!("data: {e}\n\n"))
        .collect::<String>()
}

fn delta(text: &str) -> String {
    format!(r#"{{"choices":[{{"delta":{{"content":"{text}"}}}}]}}"#)
}

/// Collect every chunk a stream produces, with cancellation off.
async fn chunks_from(server: &MockServer) -> Vec<ChatChunk> {
    let collected = Mutex::new(Vec::new());
    stream_chat(
        &client(),
        &server.uri(),
        "qwen3-30b-a3b",
        &[ChatMessage {
            role: "user".into(),
            content: "hi".into(),
        }],
        &|| false,
        |chunk| collected.lock().unwrap().push(chunk),
    )
    .await;
    collected.into_inner().unwrap()
}

#[tokio::test]
async fn a_streamed_reply_decodes_into_chunks_and_terminates() {
    let server = MockServer::start().await;
    let body = sse(&[
        r#"{"choices":[{"delta":{"role":"assistant"}}]}"#,
        &delta("Hel"),
        &delta("lo"),
        "[DONE]",
    ]);
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        // The request must ask for a stream; without it llama-server answers
        // one JSON blob and nothing decodes.
        .and(body_string_contains(r#""stream":true"#))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
        .expect(1)
        .mount(&server)
        .await;

    let chunks = chunks_from(&server).await;
    let text: String = chunks.iter().map(|c| c.delta.as_str()).collect();
    assert_eq!(text, "Hello");

    // Exactly one terminating chunk, and it is the last one -- the view
    // re-enables its input on it.
    assert_eq!(chunks.iter().filter(|c| c.done).count(), 1);
    let last = chunks.last().unwrap();
    assert!(last.done);
    assert!(last.error.is_none());
}

/// A guest that stops serving mid-generation: the partial reply is kept, and
/// the panel is told why it stopped rather than being left waiting.
#[tokio::test]
async fn a_stream_that_ends_without_done_reports_an_error() {
    let server = MockServer::start().await;
    let body = sse(&[&delta("half a sen")]);
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
        .mount(&server)
        .await;

    let chunks = chunks_from(&server).await;
    let text: String = chunks.iter().map(|c| c.delta.as_str()).collect();
    assert_eq!(text, "half a sen");
    let last = chunks.last().unwrap();
    assert!(last.done);
    assert!(last.error.is_some());
}

#[tokio::test]
async fn a_refused_completion_reports_the_server_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(400).set_body_raw(
            r#"{"error":{"message":"model not found"}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let chunks = chunks_from(&server).await;
    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].done);
    let error = chunks[0].error.clone().unwrap();
    assert!(error.contains("400"), "{error}");
    assert!(error.contains("model not found"), "{error}");
}

/// Cancelling stops the stream and still delivers the one `done` chunk, with no
/// error -- the user asked for it, so it is not a failure.
#[tokio::test]
async fn cancelling_ends_the_stream_cleanly() {
    let server = MockServer::start().await;
    let body = sse(&[&delta("a"), &delta("b"), "[DONE]"]);
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
        .mount(&server)
        .await;

    let collected = Mutex::new(Vec::new());
    stream_chat(
        &client(),
        &server.uri(),
        "m",
        &[ChatMessage {
            role: "user".into(),
            content: "hi".into(),
        }],
        &|| true,
        |chunk| collected.lock().unwrap().push(chunk),
    )
    .await;

    let chunks = collected.into_inner().unwrap();
    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].done);
    assert!(chunks[0].error.is_none());
    assert!(chunks[0].delta.is_empty());
}
