//! Engram core engine: the `.engram` document model, durable persistence, the architecture
//! graph, the Context Builder, the Context Health auditor and the C ABI used by the macOS app.

pub mod analysis;
pub mod context;
pub mod error;
pub mod ffi;
pub mod graph;
pub mod health;
pub mod render;
pub mod ops;
pub mod schema;
pub mod store;
pub mod text;
pub mod tokens;
pub mod time;

pub use context::{build_context, ContextBundle, ContextRequest};
pub use error::{EngramError, Result};
pub use health::{audit, HealthReport};
pub use graph::{Graph, Layout, LayoutOptions};
pub use ops::Op;
pub use schema::*;
pub use store::{Snapshot, Store};
pub use text::Text;
