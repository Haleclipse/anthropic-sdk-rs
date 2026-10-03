//! Maps to: `@azure/msal-common` 16.4.0 `src/network/NetworkResponse.ts`,
//! for the fields read here (not `headers`).

/// `NetworkResponse<T>`: the body as the network module parsed it, and the
/// status.
pub(crate) struct NetworkResponse<T> {
    pub status: u16,
    pub body: T,
}
