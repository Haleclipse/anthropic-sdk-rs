//! Maps to: `@azure/identity`
//! `credentials/managedIdentityCredential/imdsMsi.js`, the IMDS probe. Getting
//! the token once IMDS answers is MSAL's (`imds.rs`).

use std::time::Duration;

use super::{imds_host, map_scopes_to_resource, MsiError};
use crate::azure_identity::identity_client::{self, Request, RetryOptions};
use crate::azure_identity::js::JsTruthy;
use crate::azure_identity::Environment;

/// `:8`.
const MSI_NAME: &str = "ManagedIdentityCredential - IMDS";
/// `:10-11`.
const IMDS_HOST: &str = "http://169.254.169.254";
const IMDS_ENDPOINT_PATH: &str = "/metadata/identity/oauth2/token";

/// Maps to: `:16-34` `prepareInvalidRequestOptions`: the token path with no
/// query and no `Metadata` header, so that any IMDS answers at once with an
/// error. `resource` is mapped again, as a lone scope.
fn prepare_invalid_request_options(resource: &str, env: &Environment) -> Result<Request, MsiError> {
    if map_scopes_to_resource(&[resource.to_owned()]).is_none_or(|resource| resource.is_empty()) {
        return Err(MsiError::Other(format!(
            "{MSI_NAME}: Multiple scopes are not supported."
        )));
    }
    // `new URL(imdsEndpointPath, AZURE_POD_IDENTITY_AUTHORITY_HOST ?? imdsHost)`.
    // A set variable is empty here (a non-empty one skipped the probe), and
    // an empty base is not a URL.
    let host = match env.var("AZURE_POD_IDENTITY_AUTHORITY_HOST") {
        Some(_) => return Err(MsiError::Other("Invalid URL".to_owned())),
        None => imds_host(IMDS_HOST),
    };
    Ok(Request {
        method: reqwest::Method::GET,
        url: format!("{host}{IMDS_ENDPOINT_PATH}"),
        headers: vec![("Accept", "application/json".to_owned())],
        body: None,
        // `:63-65`: the bearer policy passes no request timeout.
        timeout: Some(Duration::from_millis(1000)),
    })
}

/// Maps to: `:42-95` `imdsMsi.isAvailable`. The client is
/// `isAvailableIdentityClient` (`index.js:88-93`): no retries and no
/// `imdsRetryPolicy`. A response is availability, unless it is Docker
/// Desktop's 403 about an unreachable network or host; a failed request is
/// unavailability. `client` is `None` when it could not be built, which fails
/// the request.
pub(super) async fn is_available(
    scopes: &[String],
    client: Option<&reqwest::Client>,
    env: &Environment,
) -> Result<bool, MsiError> {
    let Some(resource) = map_scopes_to_resource(scopes).filter(|resource| !resource.is_empty())
    else {
        return Ok(false);
    };
    // `:49-52`: a pod identity endpoint is assumed to exist.
    if env
        .var("AZURE_POD_IDENTITY_AUTHORITY_HOST")
        .truthy()
        .is_some()
    {
        return Ok(true);
    }
    let request = prepare_invalid_request_options(&resource, env)?;
    let Some(client) = client else {
        return Ok(false);
    };
    let no_retries = RetryOptions {
        max_retries: 0,
        ..RetryOptions::IDENTITY_CLIENT
    };
    Ok(
        match identity_client::send(client, &request, no_retries).await {
            Err(_) => false,
            Ok(response) => !(response.status == 403 && response.body.contains("unreachable")),
        },
    )
}
