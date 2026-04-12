// AMOS Governance Program - Instructions Module
// Exports all instruction handlers

pub mod gates;
pub mod governance;
pub mod priority;
pub mod proposals;
pub mod research;
pub mod rewards;

pub use gates::*;
pub use governance::*;
pub use priority::*;
pub use proposals::*;
pub use research::*;
pub use rewards::*;
