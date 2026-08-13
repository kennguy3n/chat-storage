//! Background job scheduler — trait that platform bridges implement.

/// Scheduler trait for background jobs.
pub trait Scheduler: Send + Sync {
    /// Schedule a background job.
    fn schedule(&self, job: ScheduledJob);
}

/// A scheduled background job.
#[derive(Debug, Clone)]
pub struct ScheduledJob {
    pub job_type: JobType,
    pub interval_ms: u64,
}

/// Types of background jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobType {
    ArchiveFlush,
    BackupIncrement,
    SearchShardBuild,
    EvictionCheck,
    EpochRotation,
}

/// No-op scheduler (for testing / desktop).
#[derive(Debug, Default)]
pub struct NoopScheduler;

impl Scheduler for NoopScheduler {
    fn schedule(&self, _job: ScheduledJob) {}
}
