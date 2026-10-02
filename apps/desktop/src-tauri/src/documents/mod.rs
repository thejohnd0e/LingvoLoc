pub mod commands;
pub mod docx;
mod domain;
pub mod epub;
pub mod fb2;
pub mod pdf;
mod segmentation;
mod store;
mod txt;
mod worker;

use crate::domain::RuntimeError;

pub use crate::services::inference_coordinator::InferenceCoordinator;
pub use domain::{
    BlockType, DocumentBlock, DocumentJob, JobState, JobTransitionError, RequestUsage,
};
pub use segmentation::{segment_block, SegmentationLimits};
pub use store::DocumentJobStore;
pub use worker::{translate_job, translate_job_with, WorkerReport};

/// Executes one document block through the shared inference gate. The caller
/// should invoke this separately for each block to yield to interactive work.
pub fn run_block<T, F>(
    coordinator: &InferenceCoordinator,
    snapshot: &str,
    operation: F,
) -> Result<T, RuntimeError>
where
    F: FnOnce() -> Result<T, RuntimeError>,
{
    coordinator.run_background(snapshot, operation)
}
