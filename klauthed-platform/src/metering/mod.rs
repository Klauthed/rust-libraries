//! Usage metering: the [`Meter`] trait and an [`InMemoryMeter`] for per-tenant
//! usage accounting (quotas, usage-based billing).

pub mod memory;
pub mod meter;

pub use memory::InMemoryMeter;
pub use meter::Meter;
