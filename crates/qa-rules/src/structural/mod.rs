mod architecture;
mod dead;
mod duplicate;
pub mod metrics;
mod sprawl;
pub use architecture::analyze as architecture;
pub use dead::analyze as dead;
pub use duplicate::analyze as duplicate;
pub use sprawl::analyze as sprawl;
