pub mod config;
pub mod engine;
pub mod extractors;
pub mod metadata;
pub mod qa;

pub use config::{LupaConfig, QaConfig, QaMode};
pub use engine::{
    BuildProgress, DoctorReport, IndexStats, LupaEngine, SearchHit, SearchOptions, SearchResult,
};
pub use qa::{provider_from_config, QaAnswer, QaCitation, QaProvider, QaRequest};
