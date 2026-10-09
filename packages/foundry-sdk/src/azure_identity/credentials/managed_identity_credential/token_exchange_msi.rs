//! Maps to: `@azure/identity`
//! `credentials/managedIdentityCredential/tokenExchangeMsi.js`, the AKS
//! workload identity flow, which the identity library runs itself rather than
//! through MSAL. Only its detection is ported; `getToken` (`:25-36`, a
//! `WorkloadIdentityCredential`) is not, and the credential reports the flow
//! unsupported instead.

use crate::azure_identity::Environment;
use crate::azure_identity::js::JsTruthy;

/// Maps to: `:15-24` `tokenExchangeMsi.isAvailable`: a client id (the
/// credential's, else `AZURE_CLIENT_ID`), `AZURE_TENANT_ID` and
/// `AZURE_FEDERATED_TOKEN_FILE`, all truthy, read when a token is asked for.
pub(super) fn is_available(client_id: Option<&str>, env: &Environment) -> bool {
    (client_id.is_some() || env.var("AZURE_CLIENT_ID").truthy().is_some())
        && env.var("AZURE_TENANT_ID").truthy().is_some()
        && env.var("AZURE_FEDERATED_TOKEN_FILE").truthy().is_some()
}
