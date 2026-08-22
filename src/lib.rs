//! x402-wallet library target: exposes the modules so integration tests
//! (tests/) can exercise the payment core without network access.

pub mod evm;
pub mod store;
pub mod utils;
pub mod x402;
