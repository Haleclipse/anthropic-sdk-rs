// Maps to: TS resources/beta/models.ts

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::client::Anthropic;
use crate::core::error::ApiError;
use crate::core::response::ApiResponse;
use crate::internal::path::encode_path_param;
use crate::internal::request_options::RequestOptions;

// ─────────────────────────────────────────────────────────────────────────────
// Response type
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaModelInfo
///
/// Information about a specific model available through the beta API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaModelInfo {
    /// Unique model identifier.
    pub id: String,

    /// RFC 3339 datetime string representing the time at which the model was
    /// released. May be set to an epoch value if the release date is unknown.
    pub created_at: String,

    /// A human-readable name for the model.
    pub display_name: String,

    /// Object type. For Models, this is always `"model"`.
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for BetaModelInfo {
    fn default() -> Self {
        Self {
            id: String::new(),
            created_at: String::new(),
            display_name: String::new(),
            type_name: "model".to_owned(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Page wrapper
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaModelInfosPage (Page<BetaModelInfo>)
///
/// A paginated list response for beta model listings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaModelInfosPage {
    /// The list of models in this page.
    pub data: Vec<BetaModelInfo>,

    /// Whether there are more pages after this one.
    #[serde(default)]
    pub has_more: bool,

    /// ID of the first item in this page (for backward pagination).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_id: Option<String>,

    /// ID of the last item in this page (for forward pagination).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Request params
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS beta/models ModelRetrieveParams
///
/// Parameters for retrieving a specific model via the beta endpoint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BetaModelRetrieveParams {
    /// Optional beta version(s) to send in the `anthropic-beta` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub betas: Option<Vec<String>>,
}

/// Maps to: TS beta/models ModelListParams
///
/// Parameters for listing models via the beta endpoint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BetaModelListParams {
    /// ID of the item to use as a cursor for backward pagination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_id: Option<String>,

    /// ID of the item to use as a cursor for forward pagination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_id: Option<String>,

    /// Maximum number of items to return per page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,

    /// Optional beta version(s) to send in the `anthropic-beta` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub betas: Option<Vec<String>>,
}

/// TS beta namespace export-name alias for `ModelRetrieveParams`.
pub type ModelRetrieveParams = BetaModelRetrieveParams;
/// TS beta namespace export-name alias for `ModelListParams`.
pub type ModelListParams = BetaModelListParams;

// ─────────────────────────────────────────────────────────────────────────────
// Resource
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS beta/models Models class
///
/// Resource for querying models through the beta API.
pub struct BetaModels<'a> {
    client: &'a Anthropic,
}

/// TS beta namespace export-name alias for the `Models` resource class.
pub type Models<'a> = BetaModels<'a>;

impl<'a> BetaModels<'a> {
    /// Create a new `BetaModels` resource bound to the given client.
    pub fn new(client: &'a Anthropic) -> Self {
        Self { client }
    }

    /// Maps to: TS Models.retrieve() -- GET /v1/models/{model_id}?beta=true
    ///
    /// Get a specific model via the beta endpoint.
    pub async fn retrieve(
        &self,
        model_id: &str,
        params: Option<&BetaModelRetrieveParams>,
    ) -> Result<BetaModelInfo, ApiError> {
        self.retrieve_with_options(model_id, params, None).await
    }

    /// Retrieve a beta model with per-request options.
    pub async fn retrieve_with_options(
        &self,
        model_id: &str,
        params: Option<&BetaModelRetrieveParams>,
        options: Option<&RequestOptions>,
    ) -> Result<BetaModelInfo, ApiError> {
        Ok(self
            .retrieve_with_response_and_options(model_id, params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `retrieve(...).withResponse()`.
    pub async fn retrieve_with_response(
        &self,
        model_id: &str,
        params: Option<&BetaModelRetrieveParams>,
    ) -> Result<ApiResponse<BetaModelInfo>, ApiError> {
        self.retrieve_with_response_and_options(model_id, params, None)
            .await
    }

    /// Retrieve a beta model returning parsed data plus raw response metadata/body.
    pub async fn retrieve_with_response_and_options(
        &self,
        model_id: &str,
        params: Option<&BetaModelRetrieveParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaModelInfo>, ApiError> {
        let model_id = encode_path_param(model_id)?;
        let path = format!("/v1/models/{model_id}?beta=true");

        let extra_headers = params.and_then(|p| {
            p.betas.as_ref().map(|b| {
                let mut h = HashMap::new();
                h.insert("anthropic-beta".to_owned(), Some(b.join(",")));
                h
            })
        });

        self.client
            .get_with_response(&path, None, extra_headers.as_ref(), options)
            .await
    }

    /// Maps to: TS Models.list() -- GET /v1/models?beta=true
    ///
    /// List available models via the beta endpoint.
    /// Returns a paginated [`BetaModelInfosPage`].
    pub async fn list(
        &self,
        params: Option<&BetaModelListParams>,
    ) -> Result<BetaModelInfosPage, ApiError> {
        self.list_with_options(params, None).await
    }

    /// List beta models with per-request options.
    pub async fn list_with_options(
        &self,
        params: Option<&BetaModelListParams>,
        options: Option<&RequestOptions>,
    ) -> Result<BetaModelInfosPage, ApiError> {
        Ok(self
            .list_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `list(...).withResponse()`.
    pub async fn list_with_response(
        &self,
        params: Option<&BetaModelListParams>,
    ) -> Result<ApiResponse<BetaModelInfosPage>, ApiError> {
        self.list_with_response_and_options(params, None).await
    }

    /// List beta models returning parsed data plus raw response metadata/body.
    pub async fn list_with_response_and_options(
        &self,
        params: Option<&BetaModelListParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaModelInfosPage>, ApiError> {
        let mut query = HashMap::new();

        if let Some(p) = params {
            if let Some(ref before_id) = p.before_id {
                query.insert("before_id".to_owned(), Some(before_id.clone()));
            }
            if let Some(ref after_id) = p.after_id {
                query.insert("after_id".to_owned(), Some(after_id.clone()));
            }
            if let Some(limit) = p.limit {
                query.insert("limit".to_owned(), Some(limit.to_string()));
            }
        }

        let extra_headers = params.and_then(|p| {
            p.betas.as_ref().map(|b| {
                let mut h = HashMap::new();
                h.insert("anthropic-beta".to_owned(), Some(b.join(",")));
                h
            })
        });

        let query_ref = if query.is_empty() { None } else { Some(&query) };

        self.client
            .get_with_response(
                "/v1/models?beta=true",
                query_ref,
                extra_headers.as_ref(),
                options,
            )
            .await
    }
}
