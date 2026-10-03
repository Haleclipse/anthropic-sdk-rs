//! Maps to: `@azure/msal-common` 16.4.0 `src/cache/entities/AccessTokenEntity.ts`,
//! for the fields the two flows read, as `createAccessTokenEntity`
//! (`cache/utils/CacheHelpers.ts:61-129`) fills them.

/// A cached access token; times in seconds. The authority, realm, home
/// account and credential type MSAL also keeps are the same for every token
/// one storage holds, and a token of any type but Bearer is not stored (see
/// `ResponseHandler`), so none of them is kept.
#[derive(Clone, Debug)]
pub(crate) struct AccessTokenEntity {
    /// The application's client id, or the managed identity's id.
    pub client_id: String,
    /// The space-separated scopes.
    pub target: String,
    pub secret: String,
    pub expires_on: f64,
    pub refresh_on: Option<f64>,
}
