//! Domain DTOs.
//!
//! Field names are picked to match the JSON shapes the RACKLOG
//! frontend already consumes (`#[serde(rename = "cat")]`,
//! `"openPOs"`, `"leadTime"`, etc.), so the API and the prototype
//! agree on the wire without a translation layer.
//!
//! Files are split by domain to keep each one small. Public
//! re-exports live below so callers continue to use the flat
//! `crate::models::Item` / `crate::models::PurchaseOrder` paths
//! without caring about the file layout.

pub mod activity;
pub mod counts;
pub mod items;
pub mod locations;
pub mod purchase_orders;
pub mod sales_orders;
pub mod status;
pub mod suppliers;
pub mod transfers;

pub use activity::ActivityEntry;
pub use counts::{Count, CountInput};
pub use items::{Item, ItemInput, Lot, StockLine, Variant};
pub use locations::{Location, LocationInput};
pub use purchase_orders::{PurchaseLine, PurchaseOrder, PurchaseOrderInput};
pub use sales_orders::{SalesLine, SalesOrder, SalesOrderInput};
pub use status::StatusSummary;
pub use suppliers::{Supplier, SupplierInput};
pub use transfers::{Transfer, TransferInput, TransferLine};
