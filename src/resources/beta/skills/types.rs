// Maps to: TS resources/beta/skills/skills.ts

use std::collections::HashMap;

use reqwest::multipart::Form;
use serde::{Deserialize, Serialize};

use crate::client::Anthropic;
use crate::core::error::ApiError;
use crate::core::response::ApiResponse;
use crate::core::uploads::Uploadable;
use crate::internal::path::encode_path_param;
use crate::internal::request_options::RequestOptions;

use super::versions::SkillVersions;

// ─────────────────────────────────────────────────────────────────────────────
// Response types
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS SkillCreateResponse / SkillRetrieveResponse / SkillListResponse
///
/// A skill object returned by the beta Skills API. Used for create, retrieve,
/// and list responses (all share the same shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillResponse {
    /// Unique identifier for the skill.
    pub id: String,

    /// ISO 8601 timestamp of when the skill was created.
    pub created_at: String,

    /// Display title for the skill (human-readable label).
    pub display_title: Option<String>,

    /// The latest version identifier for the skill.
    pub latest_version: Option<String>,

    /// Source of the skill: `"custom"` (user-created) or `"anthropic"` (built-in).
    pub source: String,

    /// Object type. For Skills, this is always `"skill"`.
    #[serde(rename = "type")]
    pub type_name: String,

    /// ISO 8601 timestamp of when the skill was last updated.
    pub updated_at: String,
}

impl Default for SkillResponse {
    fn default() -> Self {
        Self {
            id: String::new(),
            created_at: String::new(),
            display_title: None,
            latest_version: None,
            source: "custom".to_owned(),
            type_name: "skill".to_owned(),
            updated_at: String::new(),
        }
    }
}

/// Maps to: TS SkillDeleteResponse
///
/// Confirmation of a skill deletion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDeleteResponse {
    /// Unique identifier for the deleted skill.
    pub id: String,

    /// Deleted object type. For Skills, this is always `"skill_deleted"`.
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for SkillDeleteResponse {
    fn default() -> Self {
        Self {
            id: String::new(),
            type_name: "skill_deleted".to_owned(),
        }
    }
}

/// Maps to: TS SkillListResponsesPageCursor (PageCursor<SkillListResponse>)
///
/// A cursor-paginated list response for skill listings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillListPage {
    /// The list of skills in this page.
    pub data: Vec<SkillResponse>,

    /// Whether there are more pages after this one.
    #[serde(default)]
    pub has_more: bool,

    /// Cursor for the next page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page: Option<String>,
}

/// TS export-name compatibility alias for `SkillCreateResponse`.
pub type SkillCreateResponse = SkillResponse;
/// TS export-name compatibility alias for `SkillRetrieveResponse`.
pub type SkillRetrieveResponse = SkillResponse;
/// TS export-name compatibility alias for `SkillListResponse`.
pub type SkillListResponse = SkillResponse;
/// TS export-name compatibility alias for `SkillListResponsesPageCursor`.
pub type SkillListResponsesPageCursor = SkillListPage;

// ─────────────────────────────────────────────────────────────────────────────
// Request params
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS SkillCreateParams
///
/// Parameters for creating a skill via multipart/form-data.
#[derive(Debug, Clone, Default)]
pub struct SkillCreateParams {
    /// Body param: Display title for the skill.
    pub display_title: Option<String>,

    /// Body param: Files to upload for the skill.
    pub files: Option<Vec<Uploadable>>,

    /// Optional beta version(s) to send in the `anthropic-beta` header.
    pub betas: Option<Vec<String>>,
}

/// Maps to: TS SkillRetrieveParams
///
/// Parameters for retrieving a skill.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillRetrieveParams {
    /// Optional beta version(s) to send in the `anthropic-beta` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub betas: Option<Vec<String>>,
}

/// Maps to: TS SkillListParams
///
/// Parameters for listing skills.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillListParams {
    /// Filter skills by source (`"custom"` or `"anthropic"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// Maximum number of items to return per page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,

    /// Cursor page token for pagination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,

    /// Optional beta version(s) to send in the `anthropic-beta` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub betas: Option<Vec<String>>,
}

/// Maps to: TS SkillDeleteParams
///
/// Parameters for deleting a skill.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillDeleteParams {
    /// Optional beta version(s) to send in the `anthropic-beta` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub betas: Option<Vec<String>>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper
// ─────────────────────────────────────────────────────────────────────────────

/// Build the `anthropic-beta` header value, always including the
/// `skills-2025-10-02` feature flag.
fn build_skills_beta_header(betas: Option<&[String]>) -> HashMap<String, Option<String>> {
    let mut parts: Vec<String> = betas.map(|b| b.to_vec()).unwrap_or_default();
    parts.push("skills-2025-10-02".to_owned());
    let mut h = HashMap::new();
    h.insert("anthropic-beta".to_owned(), Some(parts.join(",")));
    h
}

// ─────────────────────────────────────────────────────────────────────────────
// Resource
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS beta/skills Skills class
///
/// Resource for managing skills through the beta Skills API.
pub struct Skills<'a> {
    client: &'a Anthropic,
}

impl<'a> Skills<'a> {
    /// Create a new `Skills` resource bound to the given client.
    pub fn new(client: &'a Anthropic) -> Self {
        Self { client }
    }

    /// Access the skill Versions sub-resource.
    pub fn versions(&self) -> SkillVersions<'a> {
        SkillVersions::new(self.client)
    }

    /// Maps to: TS Skills.create() -- POST /v1/skills?beta=true
    ///
    /// Create a new skill using multipart/form-data. Files are sent as
    /// repeated `files[]` fields, matching the TS SDK's form encoding.
    pub async fn create(&self, params: &SkillCreateParams) -> Result<SkillResponse, ApiError> {
        self.create_with_options(params, None).await
    }

    /// Create a new skill with per-request options.
    pub async fn create_with_options(
        &self,
        params: &SkillCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<SkillResponse, ApiError> {
        Ok(self
            .create_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.skills.create(...).withResponse()`.
    pub async fn create_with_response(
        &self,
        params: &SkillCreateParams,
    ) -> Result<ApiResponse<SkillResponse>, ApiError> {
        self.create_with_response_and_options(params, None).await
    }

    /// Create a new skill returning parsed data plus raw response metadata/body.
    pub async fn create_with_response_and_options(
        &self,
        params: &SkillCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<SkillResponse>, ApiError> {
        let headers = build_skills_beta_header(params.betas.as_deref());

        self.client
            .post_multipart_with_response(
                "/v1/skills?beta=true",
                || {
                    let mut form = Form::new();

                    if let Some(display_title) = &params.display_title {
                        form = form.text("display_title", display_title.clone());
                    }

                    if let Some(files) = &params.files {
                        for file in files {
                            // TS Skills.create passes stripFilenames=false.
                            form = form.part("files[]", file.to_part(false)?);
                        }
                    }

                    Ok(form)
                },
                Some(&headers),
                options,
            )
            .await
    }

    /// Maps to: TS Skills.retrieve() -- GET /v1/skills/{skill_id}?beta=true
    ///
    /// Retrieve a skill by ID.
    pub async fn retrieve(
        &self,
        skill_id: &str,
        params: Option<&SkillRetrieveParams>,
    ) -> Result<SkillResponse, ApiError> {
        self.retrieve_with_options(skill_id, params, None).await
    }

    /// Retrieve a skill by ID with per-request options.
    pub async fn retrieve_with_options(
        &self,
        skill_id: &str,
        params: Option<&SkillRetrieveParams>,
        options: Option<&RequestOptions>,
    ) -> Result<SkillResponse, ApiError> {
        Ok(self
            .retrieve_with_response_and_options(skill_id, params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.skills.retrieve(...).withResponse()`.
    pub async fn retrieve_with_response(
        &self,
        skill_id: &str,
        params: Option<&SkillRetrieveParams>,
    ) -> Result<ApiResponse<SkillResponse>, ApiError> {
        self.retrieve_with_response_and_options(skill_id, params, None)
            .await
    }

    /// Retrieve a skill returning parsed data plus raw response metadata/body.
    pub async fn retrieve_with_response_and_options(
        &self,
        skill_id: &str,
        params: Option<&SkillRetrieveParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<SkillResponse>, ApiError> {
        let skill_id = encode_path_param(skill_id)?;
        let path = format!("/v1/skills/{skill_id}?beta=true");
        let headers = build_skills_beta_header(params.and_then(|p| p.betas.as_deref()));

        self.client
            .get_with_response(&path, None, Some(&headers), options)
            .await
    }

    /// Maps to: TS Skills.list() -- GET /v1/skills?beta=true
    ///
    /// List skills. Returns a cursor-paginated [`SkillListPage`].
    pub async fn list(&self, params: Option<&SkillListParams>) -> Result<SkillListPage, ApiError> {
        self.list_with_options(params, None).await
    }

    /// List skills with per-request options.
    pub async fn list_with_options(
        &self,
        params: Option<&SkillListParams>,
        options: Option<&RequestOptions>,
    ) -> Result<SkillListPage, ApiError> {
        Ok(self
            .list_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.skills.list(...).withResponse()`.
    pub async fn list_with_response(
        &self,
        params: Option<&SkillListParams>,
    ) -> Result<ApiResponse<SkillListPage>, ApiError> {
        self.list_with_response_and_options(params, None).await
    }

    /// List skills returning parsed data plus raw response metadata/body.
    pub async fn list_with_response_and_options(
        &self,
        params: Option<&SkillListParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<SkillListPage>, ApiError> {
        let mut query = HashMap::new();

        if let Some(p) = params {
            if let Some(ref source) = p.source {
                query.insert("source".to_owned(), Some(source.clone()));
            }
            if let Some(limit) = p.limit {
                query.insert("limit".to_owned(), Some(limit.to_string()));
            }
            if let Some(ref page) = p.page {
                query.insert("page".to_owned(), Some(page.clone()));
            }
        }

        let headers = build_skills_beta_header(params.and_then(|p| p.betas.as_deref()));
        let query_ref = if query.is_empty() { None } else { Some(&query) };

        self.client
            .get_with_response("/v1/skills?beta=true", query_ref, Some(&headers), options)
            .await
    }

    /// Maps to: TS Skills.delete() -- DELETE /v1/skills/{skill_id}?beta=true
    ///
    /// Delete a skill.
    pub async fn delete(
        &self,
        skill_id: &str,
        params: Option<&SkillDeleteParams>,
    ) -> Result<SkillDeleteResponse, ApiError> {
        self.delete_with_options(skill_id, params, None).await
    }

    /// Delete a skill with per-request options.
    pub async fn delete_with_options(
        &self,
        skill_id: &str,
        params: Option<&SkillDeleteParams>,
        options: Option<&RequestOptions>,
    ) -> Result<SkillDeleteResponse, ApiError> {
        Ok(self
            .delete_with_response_and_options(skill_id, params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.skills.delete(...).withResponse()`.
    pub async fn delete_with_response(
        &self,
        skill_id: &str,
        params: Option<&SkillDeleteParams>,
    ) -> Result<ApiResponse<SkillDeleteResponse>, ApiError> {
        self.delete_with_response_and_options(skill_id, params, None)
            .await
    }

    /// Delete a skill returning parsed data plus raw response metadata/body.
    pub async fn delete_with_response_and_options(
        &self,
        skill_id: &str,
        params: Option<&SkillDeleteParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<SkillDeleteResponse>, ApiError> {
        let skill_id = encode_path_param(skill_id)?;
        let path = format!("/v1/skills/{skill_id}?beta=true");
        let headers = build_skills_beta_header(params.and_then(|p| p.betas.as_deref()));

        self.client
            .delete_with_response(&path, Some(&headers), options)
            .await
    }
}
