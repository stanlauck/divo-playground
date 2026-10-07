// SPDX-License-Identifier: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// ComfyUI API-format nodes, keyed by node ID. This is not a UI workflow export.
pub type Workflow = BTreeMap<String, WorkflowNode>;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkflowNode {
    pub class_type: String,
    pub inputs: BTreeMap<String, Value>,
    /// Preserve API-export fields such as `_meta`.
    #[serde(default, flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitRequest {
    pub prompt: Workflow,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_data: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SubmittedJob {
    pub prompt_id: String,
    pub number: f64,
    #[serde(default)]
    pub node_errors: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Interrupted,
    /// Absent from both history and queue; also possible during a transition.
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct JobProgress {
    pub state: JobState,
    /// Zero-based index in the pending queue, not the server's priority number.
    pub queue_position: Option<usize>,
    pub history: Option<HistoryEntry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ExecutionStatus {
    pub status_str: String,
    pub completed: bool,
    #[serde(default)]
    pub messages: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct HistoryEntry {
    pub outputs: BTreeMap<String, NodeOutputs>,
    #[serde(default)]
    pub status: Option<ExecutionStatus>,
}

impl HistoryEntry {
    /// Borrow downloadable images, audio and animated-image/video descriptors.
    pub fn files(&self) -> impl Iterator<Item = &OutputFile> {
        self.outputs
            .values()
            .flat_map(|node| node.images.iter().chain(&node.audio).chain(&node.gifs))
    }

    pub fn state(&self) -> JobState {
        let Some(status) = &self.status else {
            return JobState::Unknown;
        };
        let interrupted = status.messages.iter().any(|message| {
            message
                .as_array()
                .and_then(|items| items.first())
                .and_then(Value::as_str)
                == Some("execution_interrupted")
        });
        if interrupted {
            JobState::Interrupted
        } else if status.status_str == "error" {
            // Failed/interrupted histories may have completed=false.
            JobState::Failed
        } else if status.status_str == "success" && status.completed {
            JobState::Succeeded
        } else if status.status_str == "success" {
            JobState::Running
        } else {
            JobState::Unknown
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct NodeOutputs {
    #[serde(default)]
    pub images: Vec<OutputFile>,
    #[serde(default)]
    pub audio: Vec<OutputFile>,
    #[serde(default)]
    pub gifs: Vec<OutputFile>,
    /// Retain custom outputs, including text and custom-node metadata.
    #[serde(default, flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    Input,
    Output,
    Temp,
}

impl OutputType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
            Self::Temp => "temp",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct OutputFile {
    pub filename: String,
    #[serde(default)]
    pub subfolder: String,
    #[serde(rename = "type")]
    pub output_type: OutputType,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CancelResult {
    /// False means that the job is already terminal or unknown.
    pub cancelled: bool,
}
