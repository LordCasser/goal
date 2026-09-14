//! SQL read/write for one aggregate at a time. No cross-aggregate orchestration,
//! no event emission — that belongs to `service`.

pub mod cycles;
pub mod proposals;
pub mod repeats;
pub mod settings;
pub mod tasks;
