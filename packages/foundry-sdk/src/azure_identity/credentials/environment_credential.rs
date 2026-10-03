//! Maps to: `@azure/identity` `credentials/environmentCredential.js`.
//!
//! The environment is read once, when the credential is built. Of the three
//! credentials it can select, `ClientSecretCredential` is ported; the
//! certificate and username/password ones are not (see [`Selected`]).
//! `AZURE_ADDITIONALLY_ALLOWED_TENANTS` and
//! `AZURE_CLIENT_SEND_CERTIFICATE_CHAIN` are not read: the first reaches
//! `ClientSecretCredential` under an option name it does not read
//! (`additionallyAllowedTenantIds`), and CC requests no tenant anyway; the
//! second only matters to the certificate credential. The constructor's
//! logging of the variables found (`processEnvVars`) is left out.

use std::future::Future;

use futures::future::BoxFuture;

use super::client_secret_credential::{
    check_tenant_id, generate_msal_configuration, identity_client_post, ClientSecretCredential,
};
use crate::azure_identity::errors::AuthenticationErrorBody;
use crate::azure_identity::identity_client::{Request, Response, SendError};
use crate::azure_identity::js::JsTruthy;
use crate::azure_identity::{AccessToken, CredentialError, Environment, TokenCredential};

/// `environmentCredential.js:32` `credentialName`.
const CREDENTIAL_NAME: &str = "EnvironmentCredential";

/// `environmentCredential.js:124`.
const UNAVAILABLE: &str = "EnvironmentCredential is unavailable. No underlying credential could be used. To troubleshoot, visit https://aka.ms/azsdk/js/identity/environmentcredential/troubleshoot.";

/// Maps to: `EnvironmentCredential`.
pub struct EnvironmentCredential {
    /// `_credential`: `None` when the environment selects nothing.
    credential: Option<Selected>,
    http_client: Option<reqwest::Client>,
}

/// The credential the environment selected.
enum Selected {
    ClientSecret(ClientSecretCredential),
    /// Deviation from the npm package: `ClientCertificateCredential` (selected by
    /// `AZURE_CLIENT_CERTIFICATE_PATH`) and `UsernamePasswordCredential`
    /// (`AZURE_USERNAME` and `AZURE_PASSWORD`) are not ported. Their
    /// constructors' checks run, so the same environments fail construction,
    /// but `get_token` reports the credential unavailable and the chain goes
    /// on, where CC authenticates and a failure halts the chain. Holds the
    /// message.
    Unported(&'static str),
}

impl EnvironmentCredential {
    /// Maps to: `environmentCredential.js:70-100` `new EnvironmentCredential()`.
    /// `Err` is the message of what the constructor throws: an invalid tenant
    /// (`:78-80`), or the selected credential's own constructor failing (an
    /// authority host that is not `https:`).
    pub fn new(env: &Environment, http_client: Option<reqwest::Client>) -> Result<Self, String> {
        let tenant_id = env.var("AZURE_TENANT_ID").truthy();
        let client_id = env.var("AZURE_CLIENT_ID").truthy();
        let client_secret = env.var("AZURE_CLIENT_SECRET").truthy();
        if let Some(tenant_id) = tenant_id {
            check_tenant_id(tenant_id).map_err(|error| error.to_string())?;
        }
        let selected = |credential, http_client| {
            Ok(Self {
                credential: Some(credential),
                http_client,
            })
        };

        // `:81-85`.
        if let (Some(tenant_id), Some(client_id), Some(client_secret)) =
            (tenant_id, client_id, client_secret)
        {
            let credential = ClientSecretCredential::new(
                env,
                http_client.clone(),
                tenant_id,
                client_id,
                client_secret,
            )
            .map_err(|error| error.to_string())?;
            return selected(Selected::ClientSecret(credential), http_client);
        }
        // `:86-92`, then `:93-99`. Both constructors build an MSAL client
        // (`msalClient.js:24-49`), whose checks are all that can throw here.
        let certificate_path = env.var("AZURE_CLIENT_CERTIFICATE_PATH").truthy();
        let username = env.var("AZURE_USERNAME").truthy();
        let password = env.var("AZURE_PASSWORD").truthy();
        let unported = match (tenant_id, client_id) {
            (Some(tenant_id), Some(_)) if certificate_path.is_some() => Some((
                tenant_id,
                "EnvironmentCredential is unavailable. AZURE_CLIENT_CERTIFICATE_PATH selects ClientCertificateCredential, which anthropic-sdk-foundry does not support.",
            )),
            (Some(tenant_id), Some(_)) if username.is_some() && password.is_some() => Some((
                tenant_id,
                "EnvironmentCredential is unavailable. AZURE_USERNAME and AZURE_PASSWORD select UsernamePasswordCredential, which anthropic-sdk-foundry does not support.",
            )),
            _ => None,
        };
        if let Some((tenant_id, message)) = unported {
            generate_msal_configuration(env, tenant_id).map_err(|error| error.to_string())?;
            return selected(Selected::Unported(message), None);
        }
        Ok(Self {
            credential: None,
            http_client: None,
        })
    }

    /// Maps to: `environmentCredential.js:107-126` `getToken`, with the token
    /// request transport passed in as `send`.
    async fn get_token_with<S, F>(
        &self,
        scopes: &[String],
        send: S,
    ) -> Result<Option<AccessToken>, CredentialError>
    where
        S: Fn(Request) -> F,
        F: Future<Output = Result<Response, SendError>>,
    {
        match &self.credential {
            Some(Selected::ClientSecret(credential)) => credential
                .get_token_with(scopes, send)
                .await
                .map(Some)
                .map_err(|error| authentication_failed(&error)),
            Some(Selected::Unported(message)) => {
                Err(CredentialError::Unavailable((*message).to_owned()))
            }
            None => Err(CredentialError::Unavailable(UNAVAILABLE.to_owned())),
        }
    }
}

impl TokenCredential for EnvironmentCredential {
    fn get_token<'a>(
        &'a self,
        scopes: &'a [String],
    ) -> BoxFuture<'a, Result<Option<AccessToken>, CredentialError>> {
        Box::pin(self.get_token_with(scopes, |request| {
            identity_client_post(self.http_client.as_ref(), request)
        }))
    }
}

/// Maps to: `environmentCredential.js:115-121`: any failure of the selected
/// credential becomes `AuthenticationError(400, …)`, which halts the chain,
/// with every `More details:` cut from its message
/// (`.split("More details:").join("")`).
fn authentication_failed(error: &CredentialError) -> CredentialError {
    CredentialError::authentication(
        400,
        AuthenticationErrorBody::Response {
            error: format!(
                "{CREDENTIAL_NAME} authentication failed. To troubleshoot, visit https://aka.ms/azsdk/js/identity/environmentcredential/troubleshoot."
            ),
            error_description: error.to_string().replace("More details:", ""),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The credential built from exactly these variables.
    fn build(variables: &[(&str, &str)]) -> Result<EnvironmentCredential, String> {
        EnvironmentCredential::new(&Environment::new(variables.iter().copied()), None)
    }

    fn scopes() -> Vec<String> {
        vec!["https://cognitiveservices.azure.com/.default".to_owned()]
    }

    /// A transport that must not be reached.
    fn no_request(_: Request) -> std::future::Ready<Result<Response, SendError>> {
        panic!("no token request expected")
    }

    fn answer(
        status: u16,
        body: &'static str,
    ) -> impl Fn(Request) -> std::future::Ready<Result<Response, SendError>> {
        move |_| {
            std::future::ready(Ok(Response {
                status,
                headers: reqwest::header::HeaderMap::new(),
                body: body.to_owned(),
            }))
        }
    }

    fn selected(credential: &EnvironmentCredential) -> &'static str {
        match &credential.credential {
            Some(Selected::ClientSecret(_)) => "secret",
            Some(Selected::Unported(message)) if message.contains("Certificate") => "certificate",
            Some(Selected::Unported(_)) => "username",
            None => "none",
        }
    }

    /// `environmentCredential.js:70-100`: truthy variables pick the first
    /// complete branch; an invalid tenant or an `http:` authority host fails
    /// construction.
    #[test]
    fn construction_matches_official_branch_selection() {
        assert_eq!(selected(&build(&[]).unwrap()), "none");

        let ids = [("AZURE_TENANT_ID", "tenant"), ("AZURE_CLIENT_ID", "client")];
        let user = [("AZURE_USERNAME", "user"), ("AZURE_PASSWORD", "password")];
        let certificate = ("AZURE_CLIENT_CERTIFICATE_PATH", "/cert.pem");
        assert_eq!(
            selected(&build(&[ids[0], ids[1], user[0], user[1]]).unwrap()),
            "username"
        );
        let with_certificate = [ids[0], ids[1], user[0], user[1], certificate];
        assert_eq!(selected(&build(&with_certificate).unwrap()), "certificate");
        // An empty secret is falsy.
        let mut empty_secret = with_certificate.to_vec();
        empty_secret.push(("AZURE_CLIENT_SECRET", ""));
        assert_eq!(selected(&build(&empty_secret).unwrap()), "certificate");
        let mut secret = with_certificate.to_vec();
        secret.push(("AZURE_CLIENT_SECRET", "secret"));
        assert_eq!(selected(&build(&secret).unwrap()), "secret");

        let http_host = ("AZURE_AUTHORITY_HOST", "http://login.example.com");
        let insecure = Some("The authorityHost address must use the 'https' protocol.");
        let mut secret_http = secret.clone();
        secret_http.push(http_host);
        assert_eq!(build(&secret_http).err().as_deref(), insecure);
        let mut certificate_http = with_certificate.to_vec();
        certificate_http.push(http_host);
        assert_eq!(build(&certificate_http).err().as_deref(), insecure);

        assert_eq!(
            build(&[("AZURE_TENANT_ID", "not/a/tenant")]).err().as_deref(),
            Some(
                "Invalid tenant id provided. You can locate your tenant id by following the instructions listed here: https://learn.microsoft.com/partner-center/find-ids-and-domain-names."
            )
        );
    }

    /// `environmentCredential.js:115-124`: no credential and an unported one
    /// are unavailable; an inner failure is an `AuthenticationError` with
    /// `More details:` cut out; a token passes through.
    #[tokio::test]
    async fn get_token_matches_official_outcomes() {
        let scopes = scopes();

        let error = build(&[])
            .unwrap()
            .get_token_with(&scopes, no_request)
            .await
            .unwrap_err();
        assert_eq!(error, CredentialError::Unavailable(UNAVAILABLE.to_owned()));
        assert!(error.continues_chain());

        let ids = [("AZURE_TENANT_ID", "tenant"), ("AZURE_CLIENT_ID", "client")];
        let error = build(&[
            ids[0],
            ids[1],
            ("AZURE_CLIENT_CERTIFICATE_PATH", "/cert.pem"),
        ])
        .unwrap()
        .get_token_with(&scopes, no_request)
        .await
        .unwrap_err();
        assert!(matches!(&error, CredentialError::Unavailable(message)
            if message.contains("anthropic-sdk-foundry does not support")));
        assert!(error.continues_chain());

        let credential = build(&[ids[0], ids[1], ("AZURE_CLIENT_SECRET", "secret")]).unwrap();
        let error = credential
            .get_token_with(
                &scopes,
                answer(
                    400,
                    r#"{"error":"invalid_request","error_description":"More details: x"}"#,
                ),
            )
            .await
            .unwrap_err();
        assert!(!error.continues_chain());
        assert_eq!(
            error.to_string(),
            "EnvironmentCredential authentication failed. To troubleshoot, visit https://aka.ms/azsdk/js/identity/environmentcredential/troubleshoot. Status code: 400\n\
             More details:\n\
             invalid_request: Error(s): Not Available - Timestamp: Not Available - Description:  x - Correlation ID: Not Available - Trace ID: Not Available,"
        );

        let token = credential
            .get_token_with(
                &scopes,
                answer(200, r#"{"access_token":"tok","expires_in":3599}"#),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(token.token, "tok");

        // A multi-tenant alias fails inside MSAL, before any request.
        let common = [
            ("AZURE_TENANT_ID", "common"),
            ids[1],
            ("AZURE_CLIENT_SECRET", "secret"),
        ];
        let error = build(&common)
            .unwrap()
            .get_token_with(&scopes, no_request)
            .await
            .unwrap_err();
        assert_eq!(
            error,
            CredentialError::Authentication {
                status: 400,
                error: "EnvironmentCredential authentication failed. To troubleshoot, visit https://aka.ms/azsdk/js/identity/environmentcredential/troubleshoot.".to_owned(),
                error_description: "missing_tenant_id_error: See https://aka.ms/msal.js.errors#missing_tenant_id_error for details".to_owned(),
            }
        );
    }
}
