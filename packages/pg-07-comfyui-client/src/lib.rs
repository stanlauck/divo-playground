// SPDX-License-Identifier: MIT OR Apache-2.0

//! Async HTTP client for ComfyUI's native workflow API.
//!
//! Polling reports job phases, not WebSocket sampling percentages. Requests
//! that can enqueue duplicate work are never retried automatically.

#![doc = include_str!("../README.md")]

mod client;
mod error;
mod model;

pub use client::{Client, ClientConfig, RetryPolicy};
pub use error::{Error, Result};
pub use model::{
    CancelResult, ExecutionStatus, HistoryEntry, JobProgress, JobState, NodeOutputs, OutputFile,
    OutputType, SubmitRequest, SubmittedJob, Workflow, WorkflowNode,
};
