//! # STAB package
//!
//! This package contains the components that are used for the STAB module of the Stabilis protocol.
//! The STAB module is responsible for the creation of the STAB token, and managing its stability.
//!
//! The package consists of the following components:
//! - `stabilis_component`: The main component of the STAB module, which is responsible for the creation and management of the STAB token. It holds all state necessary to manage this.
//! - `proxy`: Interacting with the Stabilis component goes through the proxy component. This is used to:
//!     - Update the Stabilis component with new parameters / data, such as:
//!         - The interest rate
//!         - Collateral prices
//!     - Ensure that the Stabilis component is only interacted with by authorized callers.
//!     - Ensure potential upgrades to the Stabilis component can be done without disrupting the rest of the system.
//! - `flash_loans`: The flash loans component, which allows users to borrow STAB tokens from the Stabilis component.
//! - `stabilis_liquidity_pool`: The liquidity pool component, which is a STAB/XRD liquidity pool native to the Stabilis protocol. It is used to determine the price of STAB tokens.
//! - `oracle`: A component that aggregates oracle data and casts it into a form the Proxy Component is able to process.
//!
//! More information on each component can be found in their respective modules.

pub mod flash_loans;
pub mod proxy;
pub mod shared_structs;
pub mod stabilis_component;
pub mod stabilis_liquidity_pool;
pub mod oracle;