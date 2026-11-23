//! Engram core engine: the `.engram` document model, durable persistence, the architecture
//! graph, the Context Builder, the Context Health auditor and the C ABI used by the macOS app.

pub mod error;
pub mod schema;
pub mod store;
pub mod text;
pub mod time;

pub use error::{EngramError, Result};
pub use schema::*;
pub use store::{Snapshot, Store};
pub use text::Text;
