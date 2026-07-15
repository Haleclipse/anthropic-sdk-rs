// Maps to: TS core/resource.ts

use crate::client::Anthropic;

/// Base trait for all API resource classes.
///
/// Every resource (e.g. Messages, Models) holds a reference to the shared
/// [`Anthropic`] client and exposes it via this trait.
///
/// Maps to: TS `APIResource` abstract class in core/resource.ts
pub trait ApiResource {
    /// Returns a reference to the underlying [`Anthropic`] client.
    fn client(&self) -> &Anthropic;
}

/// TS-style export-name alias for [`ApiResource`].
pub use ApiResource as APIResource;
