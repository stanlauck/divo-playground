# pg-07-comfyui-client

Independent async Rust client for ComfyUI's native HTTP API. Licensed
MIT OR Apache-2.0. It does not depend on a host application.

## Input and API

`samples/submit.json` is a fully synthetic, native API-format submission:

```json
{
  "client_id": "synthetic-client",
  "prompt": {
    "1": {
      "class_type": "EmptyImage",
      "inputs": { "width": 64, "height": 64, "batch_size": 1, "color": 3368601 }
    },
    "2": {
      "class_type": "SaveImage",
      "inputs": { "images": ["1", 0], "filename_prefix": "synthetic-pg07" }
    }
  }
}
```

This is **not** the UI workflow JSON format. API-export fields such as `_meta`
are preserved. Node links remain native `[node_id, output_index]` values;
ComfyUI validates node classes, ports and execution semantics.
The submission wrapper accepts `prompt`, optional `client_id`, and optional
`extra_data`. Unsupported wrapper fields are rejected rather than silently
discarded. Extra node fields cannot override `class_type` or `inputs`.

```rust
use pg_07_comfyui_client::{Client, JobState, SubmitRequest};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let request: SubmitRequest = serde_json::from_str(include_str!("../samples/submit.json"))?;
let client = Client::new("http://127.0.0.1:8188")?;
let job = client.submit(&request).await?;

let progress = client.poll(&job.prompt_id).await?;
if progress.state == JobState::Queued {
    // progress.queue_position is the zero-based pending-queue position.
}

let history = client.wait(&job.prompt_id).await?;
for file in history.files() {
    let bytes = client.fetch_output(file).await?;
    // The caller decides whether and where to save these bounded bytes.
}
# Ok(())
# }
```

| Method | Native endpoint | Behavior |
|---|---|---|
| `submit` | `POST /prompt` | One submission attempt, returning the server's prompt ID |
| `history` | `GET /history/{id}` | Optional typed execution history |
| `poll` | History, then `GET /queue` | Queued/running/terminal/unknown phase snapshot |
| `wait` | Repeated polling | Overall deadline, including network requests and retry delays |
| `cancel` | `POST /api/jobs/{id}/cancel` | Idempotent, targeted cancellation of a pending or running job |
| `fetch_output` | `GET /view` | URL-encoded filename/subfolder/type, returned as bytes |

Polling reports phases, not per-node or sampler percentages; those require
ComfyUI's WebSocket events. An unknown job is not treated as complete: it may
have finished between the history and queue reads. Failed and interrupted
histories are terminal even when `completed` is false. Custom outputs remain
available in `NodeOutputs::extra`; `HistoryEntry::files` covers images, audio
and `gifs` descriptors.

### Cancellation compatibility

Cancellation requires a server implementing the native
`/api/jobs/{id}/cancel` endpoint. Its `cancelled: false` result means the job
is already terminal or unknown. Older servers return an HTTP error.
The client deliberately does **not** fall back to global `/interrupt` or queue
clearing, because that could cancel another client's work. No live ComfyUI
server is contacted by the tests.

## Reliability and limits

`ClientConfig` sets per-request timeout, polling interval, overall wait timeout,
maximum response size and retry policy. Defaults:

- Request timeout: 30 seconds; polling interval: 500 milliseconds.
- Overall wait deadline: 5 minutes.
- Each JSON response or downloaded file: at most 64 MiB, including chunked bodies.
- Safe operations: up to 3 attempts, exponential delay from 100 ms to 2 seconds.
- HTTP 429 and 5xx, connection failures, timeouts and broken response bodies are
  retryable. Other HTTP errors and invalid JSON are not.
- Delta-second `Retry-After` values are honored up to the configured delay cap;
  date-form values fall back to exponential backoff.

`submit` is never automatically replayed, including after a timeout or 5xx:
the server may already have enqueued the job. `client_id` is not an idempotency
key. After an ambiguous submission failure, inspect the server queue/history
rather than blindly submitting again.

Timing out or dropping `wait` only stops the local wait, not remote execution.
Call `cancel` explicitly to cancel the job. Downloads stay in memory; the
client does not create files. It rejects traversal-like output descriptors,
credential-bearing/query-bearing base URLs and malformed prompt IDs.
Reverse-proxy base-path prefixes are preserved. HTTP redirects are not followed;
requests are direct, without automatic environment/system proxy discovery.
Error messages do not include workflow contents or server response bodies.
Transport errors also strip request URLs, including filename query parameters.

## Checks

Run from this package directory:

```text
cargo test --offline --locked
cargo fmt -- --check
cargo clippy --offline --locked --all-targets -- -D warnings
```

Tests use an ephemeral mock HTTP server bound only to `127.0.0.1`, synthetic
requests and outputs, and no internet or live ComfyUI instance. As with any
Cargo project, dependencies must already be cached for `--offline` builds.

The optional example does contact the server URL **you explicitly supply**:

```text
cargo run --offline --locked --example submit -- http://127.0.0.1:8188
```

It submits the synthetic flat-color graph, waits and fetches output bytes,
without writing them to disk.

## References and license

- [Official ComfyUI server routes](https://docs.comfy.org/development/comfyui-server/comms_routes)
- [Native server implementation](https://github.com/Comfy-Org/ComfyUI/blob/master/server.py)

MIT OR Apache-2.0. See the repository's `LICENSE` and `LICENSE-APACHE`.
