//! Maps to: `@azure/identity` `credentials/clientSecretCredential.js`, and the
//! MSAL flow behind its `getToken`: `msal/nodeFlows/msalClient.js`
//! `getTokenByClientSecret`, then `@azure/msal-node` 5.1.1
//! `ConfidentialClientApplication.acquireTokenByClientCredential` and
//! `ClientCredentialClient`, on `@azure/msal-common` 16.4.0's authority
//! resolution, token protocol and throttling. The response handling,
//! in-memory token cache and helpers it shares with the managed identity flow
//! are in [`crate::azure_identity::msal`].
//!
//! CC reaches this credential only through `EnvironmentCredential`, and its
//! bearer policy always asks with `{ enableCae: true }`: one `"CAE"`
//! `ConfidentialClientApplication` per credential (`msalClient.js:92-112`),
//! whose caches are [`ConfidentialClientState`]. They live as long as the
//! credential, which `DefaultAzureCredential` builds anew with every Foundry
//! client; nothing is cached per process.
//!
//! Left out (`docs/ENV_REDESIGN.md` § C2c-2):
//! - The telemetry fields MSAL adds to the token request body (`x-client-SKU`,
//!   `x-client-VER`, `x-client-OS`, `x-client-CPU`, `x-ms-lib-capability`,
//!   `x-client-current-telemetry`, `x-client-last-telemetry`).
//! - Network authority discovery. MSAL takes the token endpoint from its
//!   hardcoded metadata for the public, US Government and sovereign clouds
//!   (`authority/AuthorityMetadata.ts`), which is ported; any other host it
//!   looks up over the network first (instance discovery, then the OpenID
//!   configuration). Such a host gets `{authority}/oauth2/v2.0/token` here,
//!   what discovery yields for an Entra ID host, without the two lookups; a
//!   host whose discovery would fail fails at the token request instead, and
//!   ADFS or CIAM authorities, whose token endpoints differ, are not supported.
//! - `pop` access tokens, which MSAL validates and signs. A token of any type
//!   other than Bearer is returned without being cached, as MSAL does.
//! - Logging.

use std::future::Future;
use std::sync::Mutex;

use futures::future::BoxFuture;
use serde_json::{Map, Value};

use crate::azure_identity::constants::DEFAULT_AUTHORITY_HOST;
use crate::azure_identity::identity_client::{self, Request, Response, RetryOptions, SendError};
use crate::azure_identity::js::{JsTruthy, js_string};
use crate::azure_identity::msal::access_token_entity::AccessTokenEntity;
use crate::azure_identity::msal::auth_error::{MsalError, MsalErrorKind};
use crate::azure_identity::msal::authentication_result::AuthenticationResult;
use crate::azure_identity::msal::cache_manager;
use crate::azure_identity::msal::client_credential_client::get_cached_authentication_result;
use crate::azure_identity::msal::constants::{
    AAD_AUTHORITIES, CacheOutcome, KNOWN_PUBLIC_CLOUDS, OIDC_DEFAULT_SCOPES,
    REGIONAL_AUTH_PUBLIC_CLOUD_SUFFIX, URL_FORM_CONTENT_TYPE,
};
use crate::azure_identity::msal::network_response::NetworkResponse;
use crate::azure_identity::msal::response_handler::{
    handle_server_token_response, js_seconds, validate_token_response,
};
use crate::azure_identity::msal::scope_set::scope_set;
use crate::azure_identity::msal::server_authorization_token_response::ServerAuthorizationTokenResponse;
use crate::azure_identity::msal::time_utils::now_seconds;
use crate::azure_identity::msal::url_string::{
    append_query_string, domain_from_url, url_components, url_string, validate_as_uri,
};
use crate::azure_identity::msal::url_utils::map_to_query_string;
use crate::azure_identity::{
    AccessToken, CredentialError, Environment, TokenCredential, now_ms, timestamp,
};

/// `util/tenantIdUtils.js:11`.
const INVALID_TENANT_ID: &str = "Invalid tenant id provided. You can locate your tenant id by following the instructions listed here: https://learn.microsoft.com/partner-center/find-ids-and-domain-names.";

/// `AuthorityMetadata.ts:20-49`: the hosts with hardcoded endpoint metadata,
/// whose token endpoint is `https://${host}/{tenantid}/oauth2/v2.0/token`.
const ENDPOINT_METADATA_HOSTS: [&str; 6] = [
    "login.microsoftonline.com",
    "login.chinacloudapi.cn",
    "login.microsoftonline.us",
    "login.sovcloud-identity.fr",
    "login.sovcloud-identity.de",
    "login.sovcloud-identity.sg",
];

/// `AuthorityMetadata.ts:53-109`: the hardcoded instance discovery metadata,
/// as `(preferred_network, aliases)`.
const INSTANCE_DISCOVERY_METADATA: [(&str, &[&str]); 8] = [
    (
        "login.microsoftonline.com",
        &[
            "login.microsoftonline.com",
            "login.windows.net",
            "login.microsoft.com",
            "sts.windows.net",
        ],
    ),
    (
        "login.partner.microsoftonline.cn",
        &["login.partner.microsoftonline.cn", "login.chinacloudapi.cn"],
    ),
    ("login.microsoftonline.de", &["login.microsoftonline.de"]),
    (
        "login.microsoftonline.us",
        &["login.microsoftonline.us", "login.usgovcloudapi.net"],
    ),
    (
        "login-us.microsoftonline.com",
        &["login-us.microsoftonline.com"],
    ),
    (
        "login.sovcloud-identity.fr",
        &["login.sovcloud-identity.fr"],
    ),
    (
        "login.sovcloud-identity.de",
        &["login.sovcloud-identity.de"],
    ),
    (
        "login.sovcloud-identity.sg",
        &["login.sovcloud-identity.sg"],
    ),
];

/// `RequestParameterBuilder.ts:479-516` `addClientCapabilitiesToClaims` for no
/// request claims and the CAE app's `clientCapabilities: ["cp1"]`
/// (`msalClient.js:104`).
const CAE_CLAIMS: &str = r#"{"access_token":{"xms_cc":{"values":["cp1"]}}}"#;

/// `ThrottlingUtils.ts:124-135` `calculateThrottleTime`. The `Retry-After`
/// it would honour is looked up as `"Retry-After"` in the header object
/// `IdentityClient` lowercases (`identityClient.js:216`), so it is never
/// found and every throttle lasts `DEFAULT_THROTTLE_TIME_SECONDS`.
const THROTTLE_TIME_MS: u64 = 60 * 1000;

/// Maps to: `ClientSecretCredential`.
pub struct ClientSecretCredential {
    tenant_id: String,
    client_id: String,
    client_secret: String,
    /// `state.msalConfig.auth.authority` (`msalClient.js:27`).
    authority: String,
    app: Mutex<ConfidentialClientState>,
    /// The `process.env` MSAL reads its region settings from.
    env: Environment,
    http_client: Option<reqwest::Client>,
}

impl ClientSecretCredential {
    /// Maps to: `clientSecretCredential.js:33-51`
    /// `new ClientSecretCredential(tenantId, clientId, clientSecret)`, with no
    /// options: `additionallyAllowedTenants` is unset (`EnvironmentCredential`
    /// passes `additionallyAllowedTenantIds`, which this constructor does not
    /// read).
    pub fn new(
        env: &Environment,
        http_client: Option<reqwest::Client>,
        tenant_id: &str,
        client_id: &str,
        client_secret: &str,
    ) -> Result<Self, CredentialError> {
        for (value, name) in [
            (tenant_id, "tenantId"),
            (client_id, "clientId"),
            (client_secret, "clientSecret"),
        ] {
            if value.is_empty() {
                return Err(CredentialError::Unavailable(format!(
                    "ClientSecretCredential: {name} is a required parameter. To troubleshoot, visit https://aka.ms/azsdk/js/identity/serviceprincipalauthentication/troubleshoot."
                )));
            }
        }
        let authority = generate_msal_configuration(env, tenant_id)?;
        Ok(Self {
            tenant_id: tenant_id.to_owned(),
            client_id: client_id.to_owned(),
            client_secret: client_secret.to_owned(),
            authority,
            app: Mutex::default(),
            env: env.clone(),
            http_client,
        })
    }

    /// Maps to: `clientSecretCredential.js:60-66` `getToken`, with the
    /// identity pipeline's POST passed in as `send`.
    /// `processMultiTenantRequest` keeps the configured tenant, since CC asks
    /// for none; `msalClient.js:224-226` routes every failure through
    /// `handleMsalError`.
    pub(super) async fn get_token_with<S, F>(
        &self,
        scopes: &[String],
        send: S,
    ) -> Result<AccessToken, CredentialError>
    where
        S: Fn(Request) -> F,
        F: Future<Output = Result<Response, SendError>>,
    {
        let result = self
            .acquire_token_by_client_credential(scopes, &send)
            .await
            .and_then(ensure_valid_msal_token);
        result.map_err(handle_msal_error)
    }

    /// Maps to: `ConfidentialClientApplication.ts:143-253`
    /// `acquireTokenByClientCredential`, called by `msalClient.js:209-214`.
    async fn acquire_token_by_client_credential<S, F>(
        &self,
        scopes: &[String],
        send: &S,
    ) -> Result<AuthenticationResult, MsalError>
    where
        S: Fn(Request) -> F,
        F: Future<Output = Result<Response, SendError>>,
    {
        // `ClientApplication.ts:586-616` `initializeBaseRequest` appends the
        // OIDC default scopes and generates the correlation id; `:166-173`
        // filters those scopes out again.
        let correlation_id = uuid::Uuid::new_v4().to_string();
        let request_scopes: Vec<String> = scopes
            .iter()
            .filter(|scope| !OIDC_DEFAULT_SCOPES.contains(&scope.as_str()))
            .cloned()
            .collect();

        // `:181-195`: the authority's first path segment, from the
        // lowercased URL, must not be a multi-tenant alias.
        let authority_url = url_string(&self.authority);
        let tenant = url_components(&authority_url)
            .path_segments
            .first()
            .copied();
        if tenant.is_some_and(|tenant| AAD_AUTHORITIES.contains(&tenant)) {
            return Err(MsalError::client_auth("missing_tenant_id_error"));
        }

        // `:224-229` `createAuthority`.
        let token_endpoint = resolve_token_endpoint(
            &self.authority,
            &self.tenant_id,
            azure_region(&self.env).as_deref(),
        )?;
        self.acquire_token(&request_scopes, &token_endpoint, &correlation_id, send)
            .await
    }

    /// Maps to: `ClientCredentialClient.ts:59-101` `acquireToken`, on the
    /// cache path (CC sends no claims and no `skipCache`). The cache is read
    /// by `getCachedAuthenticationResult` on the credential's own storage,
    /// whose client id, realm and environment are this credential's, so the
    /// scopes decide. A proactive refresh is awaited, and its result dropped:
    /// the cached token is what the call returns.
    async fn acquire_token<S, F>(
        &self,
        request_scopes: &[String],
        token_endpoint: &str,
        correlation_id: &str,
        send: &S,
    ) -> Result<AuthenticationResult, MsalError>
    where
        S: Fn(Request) -> F,
        F: Future<Output = Result<Response, SendError>>,
    {
        let (cached, last_cache_outcome) = get_cached_authentication_result(
            &self.app.lock().unwrap().access_tokens,
            &self.client_id,
            request_scopes,
        )?;
        let Some(cached) = cached else {
            return self
                .execute_token_request(request_scopes, token_endpoint, correlation_id, false, send)
                .await;
        };
        if last_cache_outcome == CacheOutcome::ProactivelyRefreshed {
            self.execute_token_request(request_scopes, token_endpoint, correlation_id, true, send)
                .await?;
        }
        Ok(cached)
    }

    /// Maps to: `ClientCredentialClient.ts:246-344` `executeTokenRequest`
    /// without an `appTokenProvider`.
    async fn execute_token_request<S, F>(
        &self,
        request_scopes: &[String],
        token_endpoint: &str,
        correlation_id: &str,
        refresh_access_token: bool,
        send: &S,
    ) -> Result<AuthenticationResult, MsalError>
    where
        S: Fn(Request) -> F,
        F: Future<Output = Result<Response, SendError>>,
    {
        // `protocol/Token.ts:72-103` `createTokenQueryParameters`.
        let query = map_to_query_string(&[("client-request-id", correlation_id)]);
        let endpoint = append_query_string(token_endpoint, &query);
        let body = self.create_token_request_body(request_scopes, correlation_id)?;
        // `TimeUtils.ts:13-16` `nowSeconds`, taken before the request.
        let req_timestamp = now_seconds();
        let response = self
            .send_post_request(request_scopes, endpoint, body, send)
            .await?;
        // `:316-317`: the body, with the response status.
        let field = |key: &str| response.body.get(key).cloned();
        let seconds = |key: &str| response.body.get(key).map(js_seconds);
        let server_token_response = ServerAuthorizationTokenResponse {
            status: response.status,
            token_type: field("token_type"),
            scope: field("scope"),
            expires_in: seconds("expires_in"),
            refresh_in: seconds("refresh_in"),
            access_token: field("access_token"),
            error: field("error"),
            error_description: field("error_description"),
            error_codes: field("error_codes"),
            suberror: field("suberror"),
            timestamp: field("timestamp"),
            trace_id: field("trace_id"),
            correlation_id: field("correlation_id"),
        };
        validate_token_response(&server_token_response, refresh_access_token)?;
        // The response handler's cache storage is the credential's own.
        handle_server_token_response(
            &server_token_response,
            &self.client_id,
            req_timestamp,
            request_scopes,
            |credential| {
                cache_manager::save_access_token(
                    &mut self.app.lock().unwrap().access_tokens,
                    credential,
                );
            },
        )
    }

    /// Maps to: `ClientCredentialClient.ts:350-430` `createTokenRequestBody`,
    /// for a client secret, without the telemetry parameters (see the module
    /// root).
    fn create_token_request_body(
        &self,
        request_scopes: &[String],
        correlation_id: &str,
    ) -> Result<String, MsalError> {
        // `RequestParameterBuilder.ts:83-101` `addScopes(…, false)`.
        let scope = scope_set(request_scopes)?.join(" ");
        Ok(map_to_query_string(&[
            ("client_id", &self.client_id),
            ("scope", &scope),
            ("grant_type", "client_credentials"),
            ("client-request-id", correlation_id),
            ("client_secret", &self.client_secret),
            ("claims", CAE_CLAIMS),
        ]))
    }

    /// Maps to: msal-common `protocol/Token.ts:158-230` `sendPostRequest`:
    /// the throttling checks around `IdentityClient.sendPostRequestAsync`
    /// (`identityClient.js:202-219`). A failure that is not an MSAL error
    /// (no response, an empty or unparseable body) is `network_error`.
    async fn send_post_request<S, F>(
        &self,
        thumbprint: &[String],
        endpoint: String,
        body: String,
        send: &S,
    ) -> Result<NetworkResponse<Map<String, Value>>, MsalError>
    where
        S: Fn(Request) -> F,
        F: Future<Output = Result<Response, SendError>>,
    {
        self.throttling_pre_process(thumbprint)?;
        // `Token.ts:36-66` `createTokenRequestHeaders`.
        let request = Request {
            method: reqwest::Method::POST,
            url: endpoint,
            headers: vec![("Content-Type", URL_FORM_CONTENT_TYPE.to_owned())],
            body: Some(body),
            timeout: None,
        };
        let response = send(request)
            .await
            .map_err(|_| MsalError::client_auth("network_error"))?;
        let body = parse_response_body(&response.body)?;
        self.throttling_post_process(thumbprint, response.status, &body);
        Ok(NetworkResponse {
            status: response.status,
            body,
        })
    }

    /// Maps to: `ThrottlingUtils.ts:33-52` `preProcess`. A live entry fails
    /// the request with the `ServerError` it recorded.
    fn throttling_pre_process(&self, thumbprint: &[String]) -> Result<(), MsalError> {
        let mut app = self.app.lock().unwrap();
        let Some(index) = app.throttling.iter().position(|(key, _)| key == thumbprint) else {
            return Ok(());
        };
        let entity = &app.throttling[index].1;
        if entity.throttle_time < now_ms() {
            app.throttling.remove(index);
            return Ok(());
        }
        // `new ServerError(value.errorCodes?.join(" ") || "", value.errorMessage, …)`.
        let error_codes = match &entity.error_codes {
            Some(Value::Array(codes)) => codes
                .iter()
                .map(|code| {
                    if code.is_null() {
                        String::new()
                    } else {
                        js_string(code)
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
            _ => String::new(),
        };
        Err(MsalError::server(
            Some(&Value::String(error_codes)),
            entity.error_message.as_ref(),
        ))
    }

    /// Maps to: `ThrottlingUtils.ts:60-87` `postProcess`: a 429 or 5xx
    /// response (after the pipeline's retries) throttles the request.
    fn throttling_post_process(
        &self,
        thumbprint: &[String],
        status: u16,
        body: &Map<String, Value>,
    ) {
        if status != 429 && !(500..600).contains(&status) {
            return;
        }
        let entity = ThrottlingEntity {
            throttle_time: now_ms() + THROTTLE_TIME_MS,
            error_codes: body.get("error_codes").cloned(),
            error_message: body.get("error_description").cloned(),
        };
        let mut app = self.app.lock().unwrap();
        app.throttling.retain(|(key, _)| key != thumbprint);
        app.throttling.push((thumbprint.to_vec(), entity));
    }
}

impl TokenCredential for ClientSecretCredential {
    fn get_token<'a>(
        &'a self,
        scopes: &'a [String],
    ) -> BoxFuture<'a, Result<Option<AccessToken>, CredentialError>> {
        Box::pin(async move {
            self.get_token_with(scopes, |request| {
                identity_client_post(self.http_client.as_ref(), request)
            })
            .await
            .map(Some)
        })
    }
}

/// The token request transport: `IdentityClient.sendPostRequestAsync`
/// (`identityClient.js:202-219`) through the identity pipeline.
pub(super) async fn identity_client_post(
    http_client: Option<&reqwest::Client>,
    request: Request,
) -> Result<Response, SendError> {
    let client = identity_client::http_client(http_client)?;
    identity_client::send(&client, &request, RetryOptions::IDENTITY_CLIENT).await
}

/// Maps to: `util/tenantIdUtils.js:9-15` `checkTenantId`:
/// `/^[0-9a-zA-Z-.]+$/` (ASCII only; `$` matches at the very end).
pub(crate) fn check_tenant_id(tenant_id: &str) -> Result<(), CredentialError> {
    let valid = !tenant_id.is_empty()
        && tenant_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '.');
    if valid {
        Ok(())
    } else {
        Err(CredentialError::Other(INVALID_TENANT_ID.to_owned()))
    }
}

/// Maps to: `msal/nodeFlows/msalClient.js:24-49` `generateMsalConfiguration`,
/// the part that can throw: `resolveTenantId` (`util/tenantIdUtils.js:19-31`)
/// checks the tenant, `getAuthority` joins it to `getAuthorityHost()`, and
/// `new IdentityClient({ authorityHost: authority })` rejects an authority
/// that does not start with `https:` (`identityClient.js:45-48`). Returns the
/// authority. Every caller has a non-empty tenant, so the `common` and
/// `organizations` fallbacks of `resolveTenantId` do not apply.
pub(super) fn generate_msal_configuration(
    env: &Environment,
    tenant_id: &str,
) -> Result<String, CredentialError> {
    check_tenant_id(tenant_id)?;
    let authority = get_authority(tenant_id, get_authority_host(env));
    identity_client::base_uri(env, Some(&authority)).map_err(CredentialError::Other)
}

/// Maps to: `msal/utils.js:43-49` `getAuthorityHost` with no
/// `options.authorityHost` (none is passed): `AZURE_AUTHORITY_HOST ??
/// DefaultAuthorityHost`. An empty value survives `??` and becomes the
/// default in [`get_authority`].
fn get_authority_host(env: &Environment) -> &str {
    env.var("AZURE_AUTHORITY_HOST")
        .unwrap_or(DEFAULT_AUTHORITY_HOST)
}

/// Maps to: `msal/utils.js:54-67` `getAuthority`. A host already ending in
/// the tenant (`new RegExp(`${tenantId}/?$`)`) is the authority as it is.
fn get_authority(tenant_id: &str, host: &str) -> String {
    let host = if host.is_empty() {
        DEFAULT_AUTHORITY_HOST
    } else {
        host
    };
    if ends_with_tenant_pattern(host, tenant_id) {
        host.to_owned()
    } else if host.ends_with('/') {
        format!("{host}{tenant_id}")
    } else {
        format!("{host}/{tenant_id}")
    }
}

/// `new RegExp(`${tenantId}/?$`).test(host)` for a checked tenant, whose one
/// metacharacter is `.`: any UTF-16 code unit but a line terminator.
fn ends_with_tenant_pattern(host: &str, tenant_id: &str) -> bool {
    let host: Vec<u16> = host.encode_utf16().collect();
    let pattern: Vec<u16> = tenant_id.encode_utf16().collect();
    let matches_before = |end: usize| {
        end >= pattern.len()
            && host[end - pattern.len()..end]
                .iter()
                .zip(&pattern)
                .all(|(unit, expected)| {
                    if *expected == u16::from(b'.') {
                        !matches!(*unit, 0x0A | 0x0D | 0x2028 | 0x2029)
                    } else {
                        unit == expected
                    }
                })
    };
    matches_before(host.len())
        || (host.last() == Some(&u16::from(b'/')) && matches_before(host.len() - 1))
}

/// The region the token endpoint is rewritten for, if any.
///
/// Maps to: `regionalAuthority.js:124-138` `calculateRegionalAuthority()`
/// (`AZURE_REGIONAL_AUTHORITY_NAME`, read with `!== undefined`), then
/// `ConfidentialClientApplication.ts:197-216` (`DisableMsalForceRegion`,
/// `MSAL_FORCE_REGION`, `REGION_NAME`), then the regional step of
/// `Authority.ts:748-797` `updateMetadataWithRegionalInformation`. For
/// `TryAutoDetect`, `RegionDiscovery.ts:50-125` `detectRegion` takes
/// `REGION_NAME`, else asks IMDS over plain http through `IdentityClient`,
/// whose `allowInsecureConnection` is false: that request always fails, so the
/// endpoints stay global.
fn azure_region(env: &Environment) -> Option<String> {
    let azure_region = env.var("AZURE_REGIONAL_AUTHORITY_NAME").map(|region| {
        if region == "AutoDiscoverRegion" {
            "AUTO_DISCOVER".to_owned()
        } else {
            region.to_owned()
        }
    });
    if azure_region.as_deref() == Some("DisableMsalForceRegion") {
        return None;
    }
    let region = match env.var("MSAL_FORCE_REGION").truthy() {
        Some(forced) if azure_region.as_deref().truthy().is_none() => Some(forced.to_owned()),
        _ => azure_region,
    }
    .truthy()?;
    if region != "TryAutoDetect" {
        return Some(region);
    }
    env.var("REGION_NAME").truthy().map(str::to_owned)
}

/// The token endpoint MSAL resolves for `authority`.
///
/// Maps to: `ClientApplication.ts:644-675` `createAuthority` →
/// `AuthorityFactory.ts:34-74` `createDiscoveredInstance` →
/// `Authority.ts:396-427` `resolveEndpointsAsync`, then the `tokenEndpoint`
/// getter (`:230-238`). The authority is validated outside the discovery's
/// `try`, so its errors surface as they are; failures inside are
/// `endpoints_resolution_error`. With one path segment `replacePath`
/// (`:325-367`) and `replaceTenant` both put that segment where the metadata
/// has `{tenantid}`; with none, `replaceTenant` writes `undefined`.
fn resolve_token_endpoint(
    authority: &str,
    tenant_id: &str,
    azure_region: Option<&str>,
) -> Result<String, MsalError> {
    let resolution_error = |_| MsalError::client_auth("endpoints_resolution_error");
    // `formatAuthorityUri` (`Authority.ts:1336-1340`), then the
    // `canonicalAuthority` setter (`:182-186`).
    let canonical = url_string(&format_authority_uri(authority));
    let host = validate_as_uri(&canonical)?
        .host_name_and_port
        .unwrap_or_default()
        .to_lowercase();

    // `updateCloudDiscoveryMetadata` (`:805-924`): an ADFS tenant's
    // authority is a known authority (`msal/utils.js:75-80`, `:1114-1125`),
    // which keeps its host; otherwise the hardcoded aliases name the
    // preferred network host. Network instance discovery is not ported: any
    // other host keeps itself, as it does when discovery does not list it.
    let known_authority = tenant_id == "adfs" && domain_from_url(authority).to_lowercase() == host;
    let preferred_network = INSTANCE_DISCOVERY_METADATA
        .iter()
        .find(|(_, aliases)| !known_authority && aliases.contains(&host.as_str()))
        .map_or(host.as_str(), |(preferred, _)| *preferred);
    let canonical = url_string(&canonical.replacen(&host, preferred_network, 1));
    let components = validate_as_uri(&canonical).map_err(resolution_error)?;
    let host = components
        .host_name_and_port
        .unwrap_or_default()
        .to_lowercase();
    let tenant = components
        .path_segments
        .first()
        .copied()
        .unwrap_or("undefined");

    // `updateEndpointMetadata` (`:502-574`): hardcoded metadata, else (not
    // ported) the OpenID configuration from the network. A region rewrites
    // either.
    let token_endpoint = if ENDPOINT_METADATA_HOSTS.contains(&host.as_str()) {
        format!("https://{host}/{{tenantid}}/oauth2/v2.0/token")
    } else {
        format!("{canonical}oauth2/v2.0/token")
    };
    let token_endpoint = match azure_region {
        Some(region) => {
            build_regional_authority_string(&token_endpoint, region).map_err(resolution_error)?
        }
        None => token_endpoint,
    };
    Ok(token_endpoint
        .replace("{tenantid}", tenant)
        .replace("{tenant}", tenant))
}

/// Maps to: `Authority.ts:1214-1241` `buildRegionalAuthorityString`, without a
/// query string.
fn build_regional_authority_string(url: &str, region: &str) -> Result<String, MsalError> {
    let url = url_string(url);
    let components = validate_as_uri(&url)?;
    let current = components.host_name_and_port.unwrap_or_default();
    let host_name_and_port = if KNOWN_PUBLIC_CLOUDS.contains(&current) {
        format!("{region}.{REGIONAL_AUTH_PUBLIC_CLOUD_SUFFIX}")
    } else {
        format!("{region}.{current}")
    };
    // `UrlString.ts:207-215` `constructAuthorityUriFromObject`.
    Ok(url_string(&format!(
        "{}//{host_name_and_port}/{}",
        components.protocol.unwrap_or_default(),
        components.path_segments.join("/")
    )))
}

/// `Authority.ts:1336-1340` `formatAuthorityUri`.
fn format_authority_uri(authority: &str) -> String {
    if authority.ends_with('/') {
        authority.to_owned()
    } else {
        format!("{authority}/")
    }
}

/// The body `sendPostRequestAsync` parses (`identityClient.js:215`):
/// `JSON.parse(bodyAsText)`, or `undefined` for an empty body. An empty,
/// unparseable or `null` body fails in `sendPostRequest` (`Token.ts:181-219`)
/// as `network_error`; any other non-object reads as one with no fields.
fn parse_response_body(text: &str) -> Result<Map<String, Value>, MsalError> {
    let network_error = || MsalError::client_auth("network_error");
    if text.is_empty() {
        return Err(network_error());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(body)) => Ok(body),
        Ok(Value::Null) | Err(_) => Err(network_error()),
        Ok(_) => Ok(Map::new()),
    }
}

/// Maps to: `msal/utils.js:136-168` `handleMsalError`: endpoint resolution is
/// unavailable, a configuration error is thrown as it is, and anything else
/// becomes `AuthenticationRequiredError` with the same message.
fn handle_msal_error(error: MsalError) -> CredentialError {
    match error.kind {
        MsalErrorKind::ClientAuth
            if error.error_code.as_deref() == Some("endpoints_resolution_error") =>
        {
            CredentialError::Unavailable(error.message)
        }
        MsalErrorKind::ClientConfiguration => CredentialError::Other(error.message),
        _ => CredentialError::AuthenticationRequired(error.message),
    }
}

/// Maps to: `msal/utils.js:18-36` `ensureValidMsalToken`, then the token
/// `msalClient.js:217-222` returns (`Date.getTime()` of its dates).
fn ensure_valid_msal_token(msal_token: AuthenticationResult) -> Result<AccessToken, MsalError> {
    let Some(expires_on) = msal_token.expires_on else {
        return Err(MsalError::other(
            "Response had no \"expiresOn\" property.".to_owned(),
        ));
    };
    if msal_token.access_token.is_empty() {
        return Err(MsalError::other(
            "Response had no \"accessToken\" property.".to_owned(),
        ));
    }
    Ok(AccessToken {
        token: msal_token.access_token,
        expires_on_timestamp: timestamp(expires_on),
        refresh_after_timestamp: msal_token.refresh_on.map(timestamp),
    })
}

/// The in-memory state of the credential's `"CAE"`
/// `ConfidentialClientApplication` (its `NodeStorage`).
#[derive(Default)]
struct ConfidentialClientState {
    access_tokens: Vec<AccessTokenEntity>,
    /// Keyed by the request scopes: the one part of the request thumbprint
    /// (`ThrottlingUtils.ts:24-26`) that varies for a given credential.
    throttling: Vec<(Vec<String>, ThrottlingEntity)>,
}

/// `ThrottlingEntity`, as `postProcess` records it.
#[derive(Debug)]
struct ThrottlingEntity {
    /// Milliseconds since the Unix epoch.
    throttle_time: u64,
    error_codes: Option<Value>,
    error_message: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azure_identity::identity_client::INSECURE_AUTHORITY_HOST;
    use std::collections::VecDeque;

    const SCOPE: &str = "https://cognitiveservices.azure.com/.default";
    const ENCODED_SCOPE: &str = "https%3A%2F%2Fcognitiveservices.azure.com%2F.default";
    const ENCODED_CAE_CLAIMS: &str =
        "%7B%22access_token%22%3A%7B%22xms_cc%22%3A%7B%22values%22%3A%5B%22cp1%22%5D%7D%7D%7D";

    fn env(variables: &[(&str, &str)]) -> Environment {
        Environment::new(variables.iter().copied())
    }

    /// A request the script answered.
    struct Sent {
        url: String,
        headers: Vec<(&'static str, String)>,
        body: String,
    }

    /// A token endpoint that answers from a list and records what it got.
    struct Script {
        sent: Mutex<Vec<Sent>>,
        replies: Mutex<VecDeque<Result<Response, SendError>>>,
    }

    impl Script {
        fn new(replies: Vec<Result<Response, SendError>>) -> Self {
            Self {
                sent: Mutex::default(),
                replies: Mutex::new(replies.into()),
            }
        }

        fn transport(
            &self,
        ) -> impl Fn(Request) -> std::future::Ready<Result<Response, SendError>> + '_ {
            move |request| {
                self.sent.lock().unwrap().push(Sent {
                    url: request.url,
                    headers: request.headers,
                    body: request.body.unwrap_or_default(),
                });
                std::future::ready(
                    self.replies
                        .lock()
                        .unwrap()
                        .pop_front()
                        .expect("an unscripted token request"),
                )
            }
        }

        fn sent_count(&self) -> usize {
            self.sent.lock().unwrap().len()
        }
    }

    fn reply(status: u16, body: &str) -> Result<Response, SendError> {
        Ok(Response {
            status,
            headers: reqwest::header::HeaderMap::new(),
            body: body.to_owned(),
        })
    }

    fn scopes() -> Vec<String> {
        vec![SCOPE.to_owned()]
    }

    fn credential(tenant: &str) -> ClientSecretCredential {
        credential_in(&Environment::default(), tenant)
    }

    fn credential_in(env: &Environment, tenant: &str) -> ClientSecretCredential {
        ClientSecretCredential::new(env, None, tenant, "client-id", "s3cr&t").unwrap()
    }

    /// `tenantIdUtils.js:9-15` and `msal/utils.js:54-67`.
    #[test]
    fn tenant_check_and_authority_match_official_helpers() {
        assert!(check_tenant_id("72f988bf-86f1-41af-91ab-2d7cd011db47").is_ok());
        assert!(check_tenant_id("contoso.onmicrosoft.com").is_ok());
        for invalid in ["", "con/toso", "tenant\n", "tenänt"] {
            assert_eq!(
                check_tenant_id(invalid).unwrap_err(),
                CredentialError::Other(INVALID_TENANT_ID.to_owned()),
                "{invalid:?}"
            );
        }
        assert_eq!(
            get_authority("tenant", DEFAULT_AUTHORITY_HOST),
            "https://login.microsoftonline.com/tenant"
        );
        assert_eq!(
            get_authority("tenant", "https://login.microsoftonline.us/"),
            "https://login.microsoftonline.us/tenant"
        );
        assert_eq!(
            get_authority("tenant", ""),
            "https://login.microsoftonline.com/tenant"
        );
        // A host already ending in the tenant, `/` or not, is kept.
        assert_eq!(
            get_authority("tenant", "https://host/tenant/"),
            "https://host/tenant/"
        );
        assert_eq!(
            get_authority("tenant", "https://host/tenant"),
            "https://host/tenant"
        );
        // The tenant is a pattern: its `.` matches any character.
        assert_eq!(get_authority("a.c", "https://host/abc"), "https://host/abc");
        assert_eq!(
            get_authority("a.c", "https://host/abd"),
            "https://host/abd/a.c"
        );
    }

    /// `clientSecretCredential.js:33-51` and `identityClient.js:45-48`.
    #[test]
    fn construction_matches_official_parameter_and_host_checks() {
        let none = Environment::default();
        let Err(error) = ClientSecretCredential::new(&none, None, "tenant", "", "secret") else {
            panic!("an empty client id is rejected");
        };
        assert_eq!(
            error,
            CredentialError::Unavailable(
                "ClientSecretCredential: clientId is a required parameter. To troubleshoot, visit https://aka.ms/azsdk/js/identity/serviceprincipalauthentication/troubleshoot.".to_owned()
            )
        );
        assert_eq!(
            credential("tenant").authority,
            "https://login.microsoftonline.com/tenant"
        );

        // `AZURE_AUTHORITY_HOST ?? default`: empty falls back in getAuthority.
        let empty_host = env(&[("AZURE_AUTHORITY_HOST", "")]);
        assert_eq!(
            credential_in(&empty_host, "tenant").authority,
            "https://login.microsoftonline.com/tenant"
        );
        let http_host = env(&[("AZURE_AUTHORITY_HOST", "http://login.example.com")]);
        let Err(error) =
            ClientSecretCredential::new(&http_host, None, "tenant", "client", "secret")
        else {
            panic!("an http authority host is rejected");
        };
        assert_eq!(
            error,
            CredentialError::Other(INSECURE_AUTHORITY_HOST.to_owned())
        );
    }

    /// `Authority.ts` endpoint resolution from the hardcoded metadata,
    /// regional rewriting, and the network fallback.
    #[test]
    fn token_endpoint_matches_official_authority_resolution() {
        let endpoint = |authority: &str, tenant: &str, region: Option<&str>| {
            resolve_token_endpoint(authority, tenant, region).map_err(|error| error.message)
        };
        // The canonical authority is lowercased.
        assert_eq!(
            endpoint(
                "https://login.microsoftonline.com/MyTenant",
                "MyTenant",
                None
            )
            .unwrap(),
            "https://login.microsoftonline.com/mytenant/oauth2/v2.0/token"
        );
        // An alias moves to its preferred network host.
        assert_eq!(
            endpoint("https://login.windows.net/t", "t", None).unwrap(),
            "https://login.microsoftonline.com/t/oauth2/v2.0/token"
        );
        assert_eq!(
            endpoint("https://login.usgovcloudapi.net/t", "t", None).unwrap(),
            "https://login.microsoftonline.us/t/oauth2/v2.0/token"
        );
        // China's preferred host has no hardcoded endpoints; neither has a
        // custom host.
        assert_eq!(
            endpoint("https://login.chinacloudapi.cn/t", "t", None).unwrap(),
            "https://login.partner.microsoftonline.cn/t/oauth2/v2.0/token"
        );
        assert_eq!(
            endpoint("https://login.example.com/t", "t", None).unwrap(),
            "https://login.example.com/t/oauth2/v2.0/token"
        );
        // An ADFS authority keeps its host.
        assert_eq!(
            endpoint("https://login.windows.net/adfs", "adfs", None).unwrap(),
            "https://login.windows.net/adfs/oauth2/v2.0/token"
        );
        // A region: public clouds move to `login.microsoft.com`, others are
        // prefixed; the result is canonicalized.
        assert_eq!(
            endpoint("https://login.microsoftonline.com/t", "t", Some("WestUS")).unwrap(),
            "https://westus.login.microsoft.com/t/oauth2/v2.0/token/"
        );
        assert_eq!(
            endpoint(
                "https://login.microsoftonline.us/t",
                "t",
                Some("usgovvirginia")
            )
            .unwrap(),
            "https://usgovvirginia.login.microsoftonline.us/t/oauth2/v2.0/token/"
        );
        // No path segment: `replaceTenant` writes `undefined`.
        assert_eq!(
            endpoint("https://login.microsoftonline.com", "com", None).unwrap(),
            "https://login.microsoftonline.com/undefined/oauth2/v2.0/token"
        );
        // The authority is validated before discovery.
        assert_eq!(
            endpoint("https:login.microsoftonline.com/t", "t", None).unwrap_err(),
            "url_parse_error: See https://aka.ms/msal.js.errors#url_parse_error for details"
        );
    }

    /// `regionalAuthority.js:124-138`, `ConfidentialClientApplication.ts:197-216`
    /// and `RegionDiscovery.ts:50-125`.
    #[test]
    fn region_matches_official_selection() {
        let region = |variables: &[(&str, &str)]| azure_region(&env(variables));
        assert_eq!(region(&[]), None);
        let name = "AZURE_REGIONAL_AUTHORITY_NAME";
        assert_eq!(region(&[(name, "westus")]).as_deref(), Some("westus"));
        assert_eq!(
            region(&[(name, "AutoDiscoverRegion")]).as_deref(),
            Some("AUTO_DISCOVER")
        );
        let forced = ("MSAL_FORCE_REGION", "eastus");
        assert_eq!(region(&[forced]).as_deref(), Some("eastus"));
        assert_eq!(region(&[forced, (name, "")]).as_deref(), Some("eastus"));
        assert_eq!(region(&[forced, (name, "DisableMsalForceRegion")]), None);
        assert_eq!(region(&[(name, "TryAutoDetect")]), None);
        assert_eq!(
            region(&[(name, "TryAutoDetect"), ("REGION_NAME", "centralus")]).as_deref(),
            Some("centralus")
        );
    }

    /// `ClientCredentialClient.ts:279-430`, `Token.ts:36-103` and
    /// `ResponseHandler.ts:405-458`.
    #[tokio::test]
    async fn token_request_matches_official_client_credential_request() {
        let script = Script::new(vec![reply(
            200,
            r#"{"token_type":"Bearer","expires_in":"3599","ext_expires_in":3599,"access_token":"tok"}"#,
        )]);
        let before = now_seconds() as i64;
        let token = credential("tenant")
            .get_token_with(&scopes(), script.transport())
            .await
            .unwrap();
        let after = now_seconds() as i64;
        assert_eq!(token.token, "tok");
        assert!(
            ((before + 3599) as u64 * 1000..=(after + 3599) as u64 * 1000)
                .contains(&token.expires_on_timestamp),
            "{token:?}"
        );
        assert_eq!(token.refresh_after_timestamp, None);

        let sent = script.sent.lock().unwrap();
        let (endpoint, query) = sent[0].url.split_once('?').unwrap();
        assert_eq!(
            endpoint,
            "https://login.microsoftonline.com/tenant/oauth2/v2.0/token"
        );
        let correlation_id = query.strip_prefix("client-request-id=").unwrap();
        assert!(
            uuid::Uuid::parse_str(correlation_id).is_ok(),
            "{correlation_id}"
        );
        assert_eq!(
            sent[0].headers,
            [("Content-Type", URL_FORM_CONTENT_TYPE.to_owned())]
        );
        assert_eq!(
            sent[0].body,
            format!(
                "client_id=client-id&scope={ENCODED_SCOPE}&grant_type=client_credentials&client-request-id={correlation_id}&client_secret=s3cr%26t&claims={ENCODED_CAE_CLAIMS}"
            )
        );
    }

    /// `ClientCredentialClient.ts:59-205`: a cached token is reused until the
    /// 5-minute renewal window; past `refreshOn` a new one is requested and
    /// the cached one still returned.
    #[tokio::test]
    async fn cache_matches_official_reuse_and_proactive_refresh() {
        let credential = credential("tenant");
        let script = Script::new(vec![
            reply(
                200,
                r#"{"access_token":"first","expires_in":3599,"refresh_in":1800}"#,
            ),
            reply(200, r#"{"access_token":"second","expires_in":3599}"#),
            reply(
                503,
                r#"{"error":"temporarily_unavailable","error_description":"busy"}"#,
            ),
            reply(200, r#"{"access_token":"third","expires_in":3599}"#),
        ]);
        let scopes = scopes();
        let get = || credential.get_token_with(&scopes, script.transport());

        let first = get().await.unwrap();
        assert_eq!(first.token, "first");
        assert!(first.refresh_after_timestamp.is_some());
        assert_eq!(get().await.unwrap(), first);
        assert_eq!(script.sent_count(), 1);

        let set_times = |expires_on: Option<f64>, refresh_on: Option<f64>| {
            let mut app = credential.app.lock().unwrap();
            let entity = &mut app.access_tokens[0];
            if let Some(expires_on) = expires_on {
                entity.expires_on = expires_on;
            }
            entity.refresh_on = refresh_on;
        };
        let now = now_seconds();
        set_times(None, Some(now - 10.0));
        // The refresh is sent, the cached token returned, the new one cached.
        assert_eq!(get().await.unwrap().token, "first");
        assert_eq!(get().await.unwrap().token, "second");
        assert_eq!(script.sent_count(), 2);

        // A refresh the server fails with a 5xx keeps the cached token.
        set_times(None, Some(now - 10.0));
        assert_eq!(get().await.unwrap().token, "second");
        assert_eq!(script.sent_count(), 3);

        // Inside the renewal window the token is requested anew (the 503
        // above throttled the request, which is lifted first).
        credential.app.lock().unwrap().throttling.clear();
        set_times(Some(now + 299.0), None);
        assert_eq!(get().await.unwrap().token, "third");
        assert_eq!(script.sent_count(), 4);
    }

    /// `ResponseHandler.ts:94-180`, `Token.ts:173-220` and
    /// `msal/utils.js:18-36`, through `handleMsalError`.
    #[tokio::test]
    async fn failures_match_official_messages() {
        let failure = |reply: Result<Response, SendError>| async move {
            let script = Script::new(vec![reply]);
            let credential = credential("tenant");
            let scopes = scopes();
            let result = credential.get_token_with(&scopes, script.transport()).await;
            result.unwrap_err()
        };
        let required = |message: &str| CredentialError::AuthenticationRequired(message.to_owned());

        assert_eq!(
            failure(reply(
                401,
                r#"{"error":"invalid_client","error_description":"AADSTS7000215: Invalid client secret provided.","error_codes":[7000215],"timestamp":"2026-10-04 01:02:03Z","trace_id":"trace","correlation_id":"corr"}"#,
            ))
            .await,
            required(
                "invalid_client: Error(s): 7000215 - Timestamp: 2026-10-04 01:02:03Z - Description: AADSTS7000215: Invalid client secret provided. - Correlation ID: corr - Trace ID: trace"
            )
        );
        assert_eq!(
            failure(reply(400, r#"{"error_description":"no code"}"#)).await,
            required(
                "undefined: Error(s): Not Available - Timestamp: Not Available - Description: no code - Correlation ID: Not Available - Trace ID: Not Available"
            )
        );
        assert_eq!(
            failure(reply(
                400,
                r#"{"error":"interaction_required","error_description":"AADSTS50079: MFA"}"#
            ))
            .await,
            required("interaction_required: AADSTS50079: MFA")
        );

        let network_error =
            required("network_error: See https://aka.ms/msal.js.errors#network_error for details");
        let refused = Err(SendError {
            code: Some("ECONNREFUSED"),
            message: "connection refused".to_owned(),
        });
        assert_eq!(failure(refused).await, network_error);
        assert_eq!(failure(reply(502, "")).await, network_error);
        assert_eq!(failure(reply(200, "<html>")).await, network_error);
        assert_eq!(failure(reply(200, "null")).await, network_error);

        assert_eq!(
            failure(reply(200, r#"{"token_type":"Bearer"}"#)).await,
            required("Response had no \"expiresOn\" property.")
        );
    }

    /// `ThrottlingUtils.ts:33-135`: a 5xx throttles the request for 60 s,
    /// failing it with the recorded error and no network call.
    #[tokio::test]
    async fn throttling_matches_official_rules() {
        let credential = credential("tenant");
        let script = Script::new(vec![
            reply(
                500,
                r#"{"error":"server_error","error_description":"AADSTS50001: down","error_codes":[50001,90002]}"#,
            ),
            reply(200, r#"{"access_token":"tok","expires_in":3599}"#),
        ]);
        let scopes = scopes();
        let get = || credential.get_token_with(&scopes, script.transport());

        assert!(matches!(
            get().await.unwrap_err(),
            CredentialError::AuthenticationRequired(message) if message.starts_with("server_error: Error(s): 50001,90002")
        ));
        {
            let app = credential.app.lock().unwrap();
            let throttle_time = app.throttling[0].1.throttle_time;
            let now = now_ms();
            assert!(
                (now + THROTTLE_TIME_MS - 1000..=now + THROTTLE_TIME_MS).contains(&throttle_time)
            );
        }
        assert_eq!(
            get().await.unwrap_err(),
            CredentialError::AuthenticationRequired("50001 90002: AADSTS50001: down".to_owned())
        );
        assert_eq!(script.sent_count(), 1);

        credential.app.lock().unwrap().throttling[0].1.throttle_time = 0;
        assert_eq!(get().await.unwrap().token, "tok");
        assert!(credential.app.lock().unwrap().throttling.is_empty());
    }

    /// `ConfidentialClientApplication.ts:181-195` and `ScopeSet.ts:30-47`:
    /// both fail before any request.
    #[tokio::test]
    async fn tenant_and_scope_errors_match_official_checks() {
        let script = Script::new(Vec::new());
        for tenant in ["common", "Organizations"] {
            assert_eq!(
                credential(tenant)
                    .get_token_with(&scopes(), script.transport())
                    .await
                    .unwrap_err(),
                CredentialError::AuthenticationRequired(
                    "missing_tenant_id_error: See https://aka.ms/msal.js.errors#missing_tenant_id_error for details".to_owned()
                )
            );
        }
        // OIDC defaults are filtered out of the request scopes.
        assert_eq!(
            credential("tenant")
                .get_token_with(&["openid".to_owned()], script.transport())
                .await
                .unwrap_err(),
            CredentialError::Other(
                "empty_input_scopes_error: See https://aka.ms/msal.js.errors#empty_input_scopes_error for details".to_owned()
            )
        );
        assert_eq!(script.sent_count(), 0);
    }
}
