// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{
    CancelResult, Error, HistoryEntry, JobProgress, JobState, OutputFile, Result, SubmitRequest,
    SubmittedJob,
};
use reqwest::{header, Request, Response, Url};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::time::{sleep, timeout};

#[derive(Clone, Debug)]
pub struct RetryPolicy {
    /// Includes the first attempt. Submission always uses exactly one attempt.
    pub max_attempts: usize,
    pub initial_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(2),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub request_timeout: Duration,
    pub poll_interval: Duration,
    pub wait_timeout: Duration,
    /// Bounds each JSON response and downloaded file, including chunked bodies.
    pub max_response_bytes: usize,
    pub retry: RetryPolicy,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(30),
            poll_interval: Duration::from_millis(500),
            wait_timeout: Duration::from_secs(300),
            max_response_bytes: 64 * 1024 * 1024,
            retry: RetryPolicy::default(),
        }
    }
}

/// Cloneable client sharing reqwest's connection pool.
#[derive(Clone)]
pub struct Client {
    base_url: Url,
    http: reqwest::Client,
    config: ClientConfig,
}

#[derive(Deserialize)]
struct Queue {
    queue_running: Vec<Vec<Value>>,
    queue_pending: Vec<Vec<Value>>,
}

impl Client {
    pub fn new(base_url: &str) -> Result<Self> {
        Self::with_config(base_url, ClientConfig::default())
    }

    pub fn with_config(base_url: &str, config: ClientConfig) -> Result<Self> {
        if config.request_timeout.is_zero()
            || config.poll_interval.is_zero()
            || config.wait_timeout.is_zero()
            || config.max_response_bytes == 0
            || config.retry.max_attempts == 0
            || config.retry.initial_delay > config.retry.max_delay
        {
            return Err(Error::InvalidConfiguration(
                "timeouts, poll interval, size limit and attempts must be positive; initial retry delay must not exceed its cap",
            ));
        }
        let mut base_url = Url::parse(base_url)
            .map_err(|_| Error::InvalidConfiguration("base URL must be an absolute HTTP(S) URL"))?;
        if !matches!(base_url.scheme(), "http" | "https")
            || base_url.host_str().is_none()
            || !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
        {
            return Err(Error::InvalidConfiguration(
                "base URL must use HTTP(S), without credentials, query or fragment",
            ));
        }
        if !base_url.path().ends_with('/') {
            base_url.set_path(&format!("{}/", base_url.path()));
        }
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()?;
        Ok(Self {
            base_url,
            http,
            config,
        })
    }

    /// Submit once. A lost response is ambiguous: blindly resubmitting can
    /// enqueue duplicate work, even when client_id is unchanged.
    pub async fn submit(&self, submission: &SubmitRequest) -> Result<SubmittedJob> {
        if submission.prompt.is_empty()
            || submission
                .prompt
                .iter()
                .any(|(id, node)| id.trim().is_empty() || node.class_type.trim().is_empty())
            || submission
                .client_id
                .as_ref()
                .is_some_and(|id| id.trim().is_empty())
        {
            return Err(Error::InvalidInput(
                "workflow, node IDs, class types and any client_id must be nonempty",
            ));
        }
        if submission
            .prompt
            .values()
            .any(|node| node.extra.contains_key("class_type") || node.extra.contains_key("inputs"))
        {
            return Err(Error::InvalidInput(
                "extra node fields must not override class_type or inputs",
            ));
        }
        let request = self
            .http
            .post(self.endpoint(&["prompt"])?)
            .json(submission)
            .build()?;
        let job: SubmittedJob = self.json(request, false).await?;
        validate_prompt_id(&job.prompt_id)
            .map_err(|_| Error::Protocol("submission returned an invalid prompt_id"))?;
        Ok(job)
    }

    pub async fn history(&self, prompt_id: &str) -> Result<Option<HistoryEntry>> {
        validate_prompt_id(prompt_id)?;
        let request = self
            .http
            .get(self.endpoint(&["history", prompt_id])?)
            .build()?;
        let mut history: BTreeMap<String, HistoryEntry> = self.json(request, true).await?;
        Ok(history.remove(prompt_id))
    }

    /// Return one phase-level progress snapshot. Unknown is not terminal,
    /// because completion can race the history and queue reads.
    pub async fn poll(&self, prompt_id: &str) -> Result<JobProgress> {
        if let Some(history) = self.history(prompt_id).await? {
            return Ok(JobProgress {
                state: history.state(),
                queue_position: None,
                history: Some(history),
            });
        }
        let request = self.http.get(self.endpoint(&["queue"])?).build()?;
        let queue: Queue = self.json(request, true).await?;
        for entry in queue.queue_running.iter().chain(&queue.queue_pending) {
            queue_id(entry)?;
        }
        let (state, queue_position) = if queue
            .queue_running
            .iter()
            .any(|entry| queue_id(entry).ok() == Some(prompt_id))
        {
            (JobState::Running, None)
        } else if let Some(index) = queue
            .queue_pending
            .iter()
            .position(|entry| queue_id(entry).ok() == Some(prompt_id))
        {
            (JobState::Queued, Some(index))
        } else {
            (JobState::Unknown, None)
        };
        Ok(JobProgress {
            state,
            queue_position,
            history: None,
        })
    }

    /// Wait under one overall deadline, including HTTP calls and retry delays.
    /// Dropping this future or timing out does not cancel the remote job.
    pub async fn wait(&self, prompt_id: &str) -> Result<HistoryEntry> {
        validate_prompt_id(prompt_id)?;
        timeout(self.config.wait_timeout, async {
            loop {
                let progress = self.poll(prompt_id).await?;
                match progress.state {
                    JobState::Succeeded => {
                        return progress
                            .history
                            .ok_or(Error::Protocol("completed job has no history"));
                    }
                    JobState::Failed => return Err(Error::ExecutionFailed),
                    JobState::Interrupted => return Err(Error::ExecutionInterrupted),
                    _ => sleep(self.config.poll_interval).await,
                }
            }
        })
        .await
        .map_err(|_| Error::WaitTimeout)?
    }

    /// Address one job via the idempotent native job-cancel endpoint. Requires
    /// a ComfyUI server implementing /api/jobs/{id}/cancel. No global interrupt
    /// or queue-clear fallback is attempted on older servers.
    pub async fn cancel(&self, prompt_id: &str) -> Result<CancelResult> {
        validate_prompt_id(prompt_id)?;
        let request = self
            .http
            .post(self.endpoint(&["api", "jobs", prompt_id, "cancel"])?)
            .build()?;
        self.json(request, true).await
    }

    /// Fetch a file as bounded bytes. This method never writes local files.
    pub async fn fetch_output(&self, file: &OutputFile) -> Result<Vec<u8>> {
        validate_file(file)?;
        let mut url = self.endpoint(&["view"])?;
        url.query_pairs_mut()
            .append_pair("filename", &file.filename)
            .append_pair("subfolder", &file.subfolder)
            .append_pair("type", file.output_type.as_str());
        let request = self.http.get(url).build()?;
        self.execute(request, true).await
    }

    fn endpoint(&self, segments: &[&str]) -> Result<Url> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .map_err(|_| Error::InvalidConfiguration("base URL cannot contain path segments"))?
            .pop_if_empty()
            .extend(segments);
        Ok(url)
    }

    async fn json<T: DeserializeOwned>(&self, request: Request, retry: bool) -> Result<T> {
        Ok(serde_json::from_slice(
            &self.execute(request, retry).await?,
        )?)
    }

    async fn execute(&self, request: Request, retry: bool) -> Result<Vec<u8>> {
        let attempts = if retry {
            self.config.retry.max_attempts
        } else {
            1
        };
        let mut delay = self.config.retry.initial_delay;
        for attempt in 1..=attempts {
            let outgoing = request
                .try_clone()
                .ok_or(Error::Protocol("request body cannot be replayed"))?;
            let mut retry_after = None;
            let result = match self.http.execute(outgoing).await {
                Ok(response) => {
                    if response.status().is_success() {
                        self.read_limited(response).await
                    } else {
                        retry_after = response
                            .headers()
                            .get(header::RETRY_AFTER)
                            .and_then(|value| value.to_str().ok())
                            .and_then(|value| value.trim().parse::<u64>().ok())
                            .map(Duration::from_secs);
                        Err(Error::Http {
                            status: response.status().as_u16(),
                        })
                    }
                }
                Err(error) => Err(error.into()),
            };
            match result {
                Ok(bytes) => return Ok(bytes),
                Err(error) if retry && attempt < attempts && retryable(&error) => {
                    sleep(
                        retry_after
                            .unwrap_or(delay)
                            .min(self.config.retry.max_delay),
                    )
                    .await;
                    delay = delay
                        .checked_mul(2)
                        .unwrap_or(self.config.retry.max_delay)
                        .min(self.config.retry.max_delay);
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("validated retry policy always makes at least one attempt")
    }

    async fn read_limited(&self, mut response: Response) -> Result<Vec<u8>> {
        let limit = self.config.max_response_bytes;
        if response
            .content_length()
            .is_some_and(|length| length > limit as u64)
        {
            return Err(Error::ResponseTooLarge { limit });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if chunk.len() > limit - bytes.len() {
                return Err(Error::ResponseTooLarge { limit });
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

fn queue_id(entry: &[Value]) -> Result<&str> {
    entry
        .get(1)
        .and_then(Value::as_str)
        .ok_or(Error::Protocol("queue entry has no string prompt_id"))
}

fn validate_prompt_id(prompt_id: &str) -> Result<()> {
    if prompt_id.is_empty()
        || !prompt_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(Error::InvalidInput(
            "prompt_id must contain only ASCII letters, digits, '-' or '_'",
        ));
    }
    Ok(())
}

fn validate_file(file: &OutputFile) -> Result<()> {
    if file.filename.is_empty()
        || file.filename.contains("..")
        || file
            .filename
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':'))
        || file
            .subfolder
            .chars()
            .any(|character| character.is_control() || matches!(character, '\\' | ':'))
        || (!file.subfolder.is_empty()
            && file
                .subfolder
                .split('/')
                .any(|part| part.is_empty() || matches!(part, "." | "..")))
    {
        return Err(Error::InvalidInput(
            "output descriptor must use a filename and relative subfolder without traversal or control characters",
        ));
    }
    Ok(())
}

fn retryable(error: &Error) -> bool {
    match error {
        Error::Http { status } => *status == 429 || (500..=599).contains(status),
        Error::Transport(error) => {
            error.is_connect() || error.is_timeout() || error.is_body() || error.is_request()
        }
        _ => false,
    }
}
