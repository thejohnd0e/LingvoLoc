use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    Heading,
    Paragraph,
    ListItem,
    TableCell,
    Caption,
    Footnote,
    Code,
    Formula,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentBlock {
    pub id: String,
    pub ordinal: i64,
    pub block_type: BlockType,
    pub source_text: String,
    pub translated_text: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Analyzing,
    Ready,
    Translating,
    Pausing,
    Paused,
    Exporting,
    Completed,
    CompletedWithWarnings,
    Interrupted,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn transition_to(self, next: Self) -> Result<Self, JobTransitionError> {
        if self == Self::Failed && next == Self::Translating {
            return Ok(next);
        }
        if self.is_terminal() {
            return Err(JobTransitionError::Terminal(self));
        }
        if !self.can_transition_to(next) {
            return Err(JobTransitionError::Invalid {
                from: self,
                to: next,
            });
        }
        Ok(next)
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        use JobState::*;
        matches!(
            (self, next),
            (Queued, Analyzing)
                | (Analyzing, Ready)
                | (Analyzing, Failed)
                | (Ready, Translating)
                | (Translating, Pausing)
                | (Translating, Exporting)
                | (Translating, Failed)
                | (Translating, Cancelled)
                | (Pausing, Paused)
                | (Paused, Translating)
                | (Interrupted, Translating)
                | (Paused, Cancelled)
                | (Failed, Translating)
                | (Exporting, Completed)
                | (Exporting, CompletedWithWarnings)
                | (Exporting, Failed)
                | (Queued, Cancelled)
                | (Analyzing, Cancelled)
        )
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::CompletedWithWarnings | Self::Failed | Self::Cancelled
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentJob {
    pub id: String,
    pub source_path: String,
    pub source_hash: String,
    pub format: String,
    pub parser_version: String,
    pub source_language: String,
    pub target_language: String,
    pub runtime_snapshot: String,
    pub configuration_version: String,
    pub state: JobState,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobTransitionError {
    Invalid { from: JobState, to: JobState },
    Terminal(JobState),
}

impl std::fmt::Display for JobTransitionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid { from, to } => {
                write!(formatter, "invalid job transition: {from:?} -> {to:?}")
            }
            Self::Terminal(state) => write!(formatter, "job is already terminal: {state:?}"),
        }
    }
}

impl std::error::Error for JobTransitionError {}

#[cfg(test)]
mod tests {
    use super::JobState;

    #[test]
    fn failed_jobs_can_resume_translation() {
        assert_eq!(
            JobState::Failed.transition_to(JobState::Translating),
            Ok(JobState::Translating)
        );
    }
}
