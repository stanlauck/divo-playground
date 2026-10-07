// SPDX-License-Identifier: MIT OR Apache-2.0

use pg_07_comfyui_client::{
    Client, ClientConfig, Error, JobState, OutputFile, OutputType, RetryPolicy, SubmitRequest,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tiny_http::{Header, Response, Server, StatusCode};

struct Step {
    method: &'static str,
    path: String,
    request: Option<Value>,
    status: u16,
    body: Vec<u8>,
    headers: Vec<Header>,
    delay: Duration,
    chunked: bool,
}

impl Step {
    fn json(method: &'static str, path: &str, request: Option<Value>, body: Value) -> Self {
        Self {
            method,
            path: path.into(),
            request,
            status: 200,
            body: serde_json::to_vec(&body).expect("synthetic JSON should serialize"),
            headers: vec![header("Content-Type", "application/json")],
            delay: Duration::ZERO,
            chunked: false,
        }
    }

    fn get(path: &str, body: Value) -> Self {
        Self::json("GET", path, None, body)
    }

    fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name, value).expect("synthetic header should be valid")
}

struct Mock {
    url: String,
    worker: Option<JoinHandle<()>>,
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

impl Mock {
    fn new(steps: Vec<Step>) -> Self {
        let server = Server::http("127.0.0.1:0").expect("loopback server should bind");
        let url = format!("http://{}", server.server_addr());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let worker_seen = seen.clone();
        let worker = thread::spawn(move || {
            for step in steps {
                let mut request = server
                    .recv_timeout(Duration::from_secs(10))
                    .expect("mock request read should succeed")
                    .expect("expected mock request should arrive");
                assert_eq!(request.method().as_str(), step.method);
                assert_eq!(request.url(), step.path);
                worker_seen
                    .lock()
                    .expect("request log lock")
                    .push((step.method.into(), step.path.clone()));
                let mut body = Vec::new();
                request
                    .as_reader()
                    .read_to_end(&mut body)
                    .expect("request body should read");
                match step.request {
                    Some(expected) => {
                        assert_eq!(
                            serde_json::from_slice::<Value>(&body).expect("JSON request"),
                            expected
                        );
                    }
                    None => assert!(body.is_empty(), "this request must not send a body"),
                }
                thread::sleep(step.delay);
                let length = (!step.chunked).then_some(step.body.len());
                let response = Response::new(
                    StatusCode(step.status),
                    step.headers,
                    Cursor::new(step.body),
                    length,
                    None,
                );
                // A timeout/size limit can drop the connection intentionally.
                let _ = request.respond(response);
            }
            // Detect accidental replay after terminal responses as well.
            assert!(
                server
                    .recv_timeout(Duration::from_millis(50))
                    .expect("final mock receive")
                    .is_none(),
                "client sent an unexpected extra request"
            );
        });
        Self {
            url,
            worker: Some(worker),
            seen,
        }
    }

    fn client(&self) -> Client {
        Client::with_config(&self.url, config()).expect("mock client should construct")
    }

    fn finish(mut self) -> Vec<(String, String)> {
        self.worker
            .take()
            .expect("mock worker exists")
            .join()
            .expect("mock expectations should pass");
        let requests = self.seen.lock().expect("request log lock").clone();
        requests
    }
}

fn config() -> ClientConfig {
    ClientConfig {
        request_timeout: Duration::from_secs(2),
        poll_interval: Duration::from_millis(1),
        wait_timeout: Duration::from_secs(2),
        max_response_bytes: 4096,
        retry: RetryPolicy {
            max_attempts: 3,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(2),
        },
    }
}

fn submission() -> SubmitRequest {
    serde_json::from_str(include_str!("../samples/submit.json")).expect("synthetic sample")
}

fn history(status: &str, completed: bool, messages: Value) -> Value {
    json!({
        "job-1": {
            "outputs": {
                "2": {
                    "images": [{"filename":"synthetic.png","subfolder":"","type":"output"}],
                    "text": ["synthetic metadata"]
                }
            },
            "status": {
                "status_str": status,
                "completed": completed,
                "messages": messages
            }
        }
    })
}

fn file() -> OutputFile {
    OutputFile {
        filename: "synthetic.png".into(),
        subfolder: String::new(),
        output_type: OutputType::Output,
    }
}

#[tokio::test]
async fn submits_native_sample_once_preserving_meta_and_links() {
    let request = submission();
    let expected = serde_json::to_value(&request).expect("sample JSON");
    let mock = Mock::new(vec![Step::json(
        "POST",
        "/prompt",
        Some(expected),
        json!({"prompt_id":"job-1","number":7,"node_errors":{}}),
    )]);
    let result = mock.client().submit(&request).await.expect("submission");
    assert_eq!(result.prompt_id, "job-1");
    assert_eq!(result.number, 7.0);
    assert_eq!(mock.finish().len(), 1);
}

#[tokio::test]
async fn reports_pending_queue_position_and_running_state() {
    let mock = Mock::new(vec![
        Step::get("/history/job-1", json!({})),
        Step::get(
            "/queue",
            json!({"queue_running":[], "queue_pending":[[8,"other"],[9,"job-1"]]}),
        ),
        Step::get("/history/job-1", json!({})),
        Step::get(
            "/queue",
            json!({"queue_running":[[9,"job-1"]], "queue_pending":[]}),
        ),
    ]);
    let client = mock.client();
    let pending = client.poll("job-1").await.expect("pending snapshot");
    assert_eq!(pending.state, JobState::Queued);
    assert_eq!(pending.queue_position, Some(1));
    let running = client.poll("job-1").await.expect("running snapshot");
    assert_eq!(running.state, JobState::Running);
    assert_eq!(running.queue_position, None);
    mock.finish();
}

#[tokio::test]
async fn waits_across_a_queue_history_transition_and_returns_outputs() {
    let mock = Mock::new(vec![
        Step::get("/history/job-1", json!({})),
        Step::get("/queue", json!({"queue_running":[], "queue_pending":[]})),
        Step::get("/history/job-1", history("success", true, json!([]))),
    ]);
    let result = mock.client().wait("job-1").await.expect("finished job");
    assert_eq!(result.files().count(), 1);
    assert_eq!(
        result.outputs["2"].extra["text"],
        json!(["synthetic metadata"])
    );
    mock.finish();
}

#[tokio::test]
async fn failed_history_is_terminal_even_when_completed_is_false() {
    let mock = Mock::new(vec![Step::get(
        "/history/job-1",
        history(
            "error",
            false,
            json!([["execution_error", {"node_id":"2"}]]),
        ),
    )]);
    assert!(matches!(
        mock.client().wait("job-1").await,
        Err(Error::ExecutionFailed)
    ));
    mock.finish();
}

#[tokio::test]
async fn interrupted_history_is_not_reported_as_generic_failure() {
    let mock = Mock::new(vec![Step::get(
        "/history/job-1",
        history(
            "error",
            false,
            json!([["execution_interrupted", {"prompt_id":"job-1"}]]),
        ),
    )]);
    assert!(matches!(
        mock.client().wait("job-1").await,
        Err(Error::ExecutionInterrupted)
    ));
    mock.finish();
}

#[tokio::test]
async fn polls_legacy_history_without_status_as_unknown_not_complete() {
    let mock = Mock::new(vec![Step::get(
        "/history/job-1",
        json!({"job-1":{"outputs":{}}}),
    )]);
    let result = mock.client().poll("job-1").await.expect("legacy history");
    assert_eq!(result.state, JobState::Unknown);
    mock.finish();
}

#[tokio::test]
async fn wait_deadline_includes_an_in_flight_http_request() {
    let mut step = Step::get("/history/job-1", json!({}));
    step.delay = Duration::from_millis(250);
    let mock = Mock::new(vec![step]);
    let mut options = config();
    options.wait_timeout = Duration::from_millis(100);
    let client = Client::with_config(&mock.url, options).expect("timeout client");
    let start = Instant::now();
    assert!(matches!(
        client.wait("job-1").await,
        Err(Error::WaitTimeout)
    ));
    assert!(start.elapsed() < Duration::from_secs(1));
    mock.finish();
}

#[tokio::test]
async fn retries_safe_reads_on_5xx_and_429_with_bounded_retry_after() {
    let mut throttled = Step::get("/history/job-1", json!({})).status(429);
    throttled.headers.push(header("Retry-After", "3600"));
    let mock = Mock::new(vec![
        Step::get("/history/job-1", json!({})).status(503),
        throttled,
        Step::get("/history/job-1", history("success", true, json!([]))),
    ]);
    assert!(mock
        .client()
        .history("job-1")
        .await
        .expect("retried history")
        .is_some());
    assert_eq!(mock.finish().len(), 3);
}

#[tokio::test]
async fn retry_attempt_budget_is_finite() {
    let mock = Mock::new(
        (0..3)
            .map(|_| Step::get("/history/job-1", json!({})).status(502))
            .collect(),
    );
    assert!(matches!(
        mock.client().history("job-1").await,
        Err(Error::Http { status: 502 })
    ));
    assert_eq!(mock.finish().len(), 3);
}

#[tokio::test]
async fn submission_is_not_replayed_on_server_failure() {
    let request = submission();
    let mock = Mock::new(vec![Step::json(
        "POST",
        "/prompt",
        Some(serde_json::to_value(&request).expect("sample JSON")),
        json!({"error":"synthetic failure"}),
    )
    .status(503)]);
    assert!(matches!(
        mock.client().submit(&request).await,
        Err(Error::Http { status: 503 })
    ));
    assert_eq!(mock.finish().len(), 1);
}

#[tokio::test]
async fn submission_is_not_replayed_after_ambiguous_timeout() {
    let request = submission();
    let mut step = Step::json(
        "POST",
        "/prompt",
        Some(serde_json::to_value(&request).expect("sample JSON")),
        json!({"prompt_id":"job-1","number":1,"node_errors":{}}),
    );
    step.delay = Duration::from_millis(150);
    let mock = Mock::new(vec![step]);
    let mut options = config();
    options.request_timeout = Duration::from_millis(50);
    let client = Client::with_config(&mock.url, options).expect("timeout client");
    assert!(matches!(
        client.submit(&request).await,
        Err(Error::Transport(_))
    ));
    assert_eq!(mock.finish().len(), 1);
}

#[tokio::test]
async fn client_errors_and_invalid_json_are_not_retried() {
    let mock = Mock::new(vec![Step::get("/history/job-1", json!({})).status(400)]);
    assert!(matches!(
        mock.client().history("job-1").await,
        Err(Error::Http { status: 400 })
    ));
    assert_eq!(mock.finish().len(), 1);

    let mut invalid = Step::get("/history/job-1", json!({}));
    invalid.body = b"not-json".to_vec();
    let mock = Mock::new(vec![invalid]);
    assert!(matches!(
        mock.client().history("job-1").await,
        Err(Error::Decode(_))
    ));
    assert_eq!(mock.finish().len(), 1);
}

#[tokio::test]
async fn cancellation_is_targeted_and_safe_to_retry() {
    let mock = Mock::new(vec![
        Step::json("POST", "/api/jobs/job-1/cancel", None, json!({})).status(503),
        Step::json(
            "POST",
            "/api/jobs/job-1/cancel",
            None,
            json!({"cancelled":true}),
        ),
        Step::json(
            "POST",
            "/api/jobs/job-1/cancel",
            None,
            json!({"cancelled":false}),
        ),
    ]);
    let client = mock.client();
    assert!(
        client
            .cancel("job-1")
            .await
            .expect("cancel active job")
            .cancelled
    );
    assert!(
        !client
            .cancel("job-1")
            .await
            .expect("cancel terminal job")
            .cancelled
    );
    assert_eq!(mock.finish().len(), 3);
}

#[tokio::test]
async fn unsupported_cancel_does_not_fall_back_to_global_interrupt() {
    let mock = Mock::new(vec![Step::json(
        "POST",
        "/api/jobs/job-1/cancel",
        None,
        json!({}),
    )
    .status(404)]);
    assert!(matches!(
        mock.client().cancel("job-1").await,
        Err(Error::Http { status: 404 })
    ));
    assert_eq!(mock.finish().len(), 1);
}

#[tokio::test]
async fn output_parameters_are_encoded_and_binary_bytes_preserved() {
    let descriptor = OutputFile {
        filename: "synthetic & #.png".into(),
        subfolder: "set one/scene&two".into(),
        output_type: OutputType::Temp,
    };
    let expected = vec![0, 1, 2, 255, 13, 10];
    let mut response = Step::get(
        "/view?filename=synthetic+%26+%23.png&subfolder=set+one%2Fscene%26two&type=temp",
        json!({}),
    );
    response.body = expected.clone();
    let mock = Mock::new(vec![response]);
    assert_eq!(
        mock.client()
            .fetch_output(&descriptor)
            .await
            .expect("binary output"),
        expected
    );
    mock.finish();
}

#[tokio::test]
async fn response_limit_applies_to_content_length_and_chunked_bodies() {
    for chunked in [false, true] {
        let mut response = Step::get(
            "/view?filename=synthetic.png&subfolder=&type=output",
            json!({}),
        );
        response.body = vec![7; 100];
        response.chunked = chunked;
        let mock = Mock::new(vec![response]);
        let mut options = config();
        options.max_response_bytes = 16;
        let client = Client::with_config(&mock.url, options).expect("bounded client");
        assert!(matches!(
            client.fetch_output(&file()).await,
            Err(Error::ResponseTooLarge { limit: 16 })
        ));
        assert_eq!(mock.finish().len(), 1);
    }
}

#[tokio::test]
async fn reverse_proxy_prefix_is_preserved_for_all_endpoint_segments() {
    let mock = Mock::new(vec![Step::get("/comfy/history/job-1", json!({}))]);
    let client =
        Client::with_config(&format!("{}/comfy", mock.url), config()).expect("prefixed client");
    assert!(client.history("job-1").await.expect("history").is_none());
    mock.finish();
}

#[tokio::test]
async fn redirects_are_not_followed() {
    let mut response = Step::get("/history/job-1", json!({})).status(302);
    response.headers.push(header("Location", "/unexpected"));
    let mock = Mock::new(vec![response]);
    assert!(matches!(
        mock.client().history("job-1").await,
        Err(Error::Http { status: 302 })
    ));
    assert_eq!(mock.finish().len(), 1);
}

#[tokio::test]
async fn malformed_queue_and_submission_ids_fail_without_silent_success() {
    let mock = Mock::new(vec![
        Step::get("/history/job-1", json!({})),
        Step::get("/queue", json!({"queue_running":[[1]],"queue_pending":[]})),
    ]);
    assert!(matches!(
        mock.client().poll("job-1").await,
        Err(Error::Protocol(_))
    ));
    mock.finish();

    let request = submission();
    let mock = Mock::new(vec![Step::json(
        "POST",
        "/prompt",
        Some(serde_json::to_value(&request).expect("sample JSON")),
        json!({"prompt_id":"../other","number":1,"node_errors":{}}),
    )]);
    assert!(matches!(
        mock.client().submit(&request).await,
        Err(Error::Protocol(_))
    ));
    mock.finish();
}

#[tokio::test]
async fn invalid_input_is_rejected_before_any_network_request() {
    let mock = Mock::new(Vec::new());
    let client = mock.client();
    let empty = SubmitRequest {
        prompt: BTreeMap::new(),
        client_id: None,
        extra_data: BTreeMap::new(),
    };
    assert!(matches!(
        client.submit(&empty).await,
        Err(Error::InvalidInput(_))
    ));
    let mut collision = submission();
    collision
        .prompt
        .get_mut("1")
        .expect("sample node")
        .extra
        .insert("class_type".into(), json!("unexpected"));
    assert!(matches!(
        client.submit(&collision).await,
        Err(Error::InvalidInput(_))
    ));
    for prompt_id in ["", "../other", "job?x", "job\n1"] {
        assert!(matches!(
            client.history(prompt_id).await,
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            client.cancel(prompt_id).await,
            Err(Error::InvalidInput(_))
        ));
    }
    for filename in [
        "../secret.png",
        "/absolute.png",
        "a\\b.png",
        "C:secret.png",
        "bad\n.png",
    ] {
        let mut output = file();
        output.filename = filename.into();
        assert!(matches!(
            client.fetch_output(&output).await,
            Err(Error::InvalidInput(_))
        ));
    }
    for subfolder in ["../secret", "/absolute", "a/../b", "a\\b", "a//b"] {
        let mut output = file();
        output.subfolder = subfolder.into();
        assert!(matches!(
            client.fetch_output(&output).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(mock.finish().is_empty());
}

#[test]
fn configuration_rejects_invalid_urls_and_zero_limits() {
    for base in [
        "not a URL",
        "file:///synthetic",
        "http://synthetic.invalid/?mode=x",
        "http://synthetic.invalid/#section",
        "http://user@synthetic.invalid/",
    ] {
        assert!(matches!(
            Client::with_config(base, config()),
            Err(Error::InvalidConfiguration(_))
        ));
    }
    let mut options = config();
    options.retry.max_attempts = 0;
    assert!(matches!(
        Client::with_config("http://127.0.0.1:8188", options),
        Err(Error::InvalidConfiguration(_))
    ));
    let mut options = config();
    options.max_response_bytes = 0;
    assert!(matches!(
        Client::with_config("http://127.0.0.1:8188", options),
        Err(Error::InvalidConfiguration(_))
    ));
}

#[tokio::test]
async fn wait_deadline_also_limits_retry_backoff() {
    let mock = Mock::new(vec![Step::get("/history/job-1", json!({})).status(503)]);
    let mut options = config();
    options.wait_timeout = Duration::from_millis(100);
    options.retry.initial_delay = Duration::from_secs(1);
    options.retry.max_delay = Duration::from_secs(1);
    let client = Client::with_config(&mock.url, options).expect("deadline client");
    let start = Instant::now();
    assert!(matches!(
        client.wait("job-1").await,
        Err(Error::WaitTimeout)
    ));
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(mock.finish().len(), 1);
}

#[tokio::test]
async fn history_preserves_multiple_media_kinds_and_custom_metadata() {
    let mock = Mock::new(vec![Step::get(
        "/history/job-1",
        json!({
            "job-1": {
                "outputs": {
                    "2": {
                        "images": [{"filename":"synthetic.png","subfolder":"","type":"output"}],
                        "audio": [{"filename":"synthetic.wav","subfolder":"sound","type":"output"}],
                        "gifs": [{"filename":"synthetic.webm","subfolder":"","type":"temp"}],
                        "custom_data": {"synthetic": true}
                    }
                },
                "status": {"status_str":"success","completed":true,"messages":[]}
            }
        }),
    )]);
    let result = mock
        .client()
        .history("job-1")
        .await
        .expect("media history")
        .expect("job history exists");
    assert_eq!(result.files().count(), 3);
    assert_eq!(result.outputs["2"].extra["custom_data"]["synthetic"], true);
    let serialized = serde_json::to_value(&result).expect("history JSON");
    assert_eq!(serialized["outputs"]["2"]["gifs"][0]["type"], json!("temp"));
    mock.finish();
}

#[tokio::test]
async fn output_downloads_retry_but_never_accept_oversized_json() {
    let path = "/view?filename=synthetic.png&subfolder=&type=output";
    let mut output = Step::get(path, json!({}));
    output.body = vec![0, 1, 2, 255];
    let mock = Mock::new(vec![Step::get(path, json!({})).status(500), output]);
    assert_eq!(
        mock.client()
            .fetch_output(&file())
            .await
            .expect("retried output"),
        [0, 1, 2, 255]
    );
    assert_eq!(mock.finish().len(), 2);

    let mock = Mock::new(vec![Step::get(
        "/history/job-1",
        history("success", true, json!([])),
    )]);
    let mut options = config();
    options.max_response_bytes = 8;
    let client = Client::with_config(&mock.url, options).expect("bounded JSON client");
    assert!(matches!(
        client.history("job-1").await,
        Err(Error::ResponseTooLarge { limit: 8 })
    ));
    assert_eq!(mock.finish().len(), 1);
}

#[tokio::test]
async fn transport_errors_do_not_expose_request_urls() {
    let mut response = Step::get(
        "/view?filename=synthetic.png&subfolder=&type=output",
        json!({}),
    );
    response.delay = Duration::from_millis(150);
    let mock = Mock::new(vec![response]);
    let mut options = config();
    options.request_timeout = Duration::from_millis(50);
    options.retry.max_attempts = 1;
    let client = Client::with_config(&mock.url, options).expect("timeout client");
    let error = client
        .fetch_output(&file())
        .await
        .expect_err("delayed response must time out");
    let Error::Transport(ref cause) = error else {
        panic!("expected transport error");
    };
    assert!(cause.url().is_none());
    assert!(!format!("{error:?}").contains("synthetic.png"));
    assert!(!error.to_string().contains(&mock.url));
    mock.finish();
}

#[test]
fn submission_json_round_trips_and_rejects_unsupported_wrapper_fields() {
    let request = submission();
    let json = serde_json::to_string(&request).expect("sample JSON");
    assert_eq!(
        serde_json::from_str::<SubmitRequest>(&json).expect("round trip"),
        request
    );
    let mut unsupported = serde_json::to_value(&request).expect("sample JSON");
    unsupported["front"] = json!(true);
    assert!(serde_json::from_value::<SubmitRequest>(unsupported).is_err());
}
