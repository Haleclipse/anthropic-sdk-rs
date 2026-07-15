// Maps to: TS resources/beta/skills/versions.ts

use std::collections::HashMap;

use reqwest::multipart::Form;
use serde::{Deserialize, Serialize};

use crate::client::Anthropic;
use crate::core::error::ApiError;
use crate::core::response::ApiResponse;
use crate::core::uploads::Uploadable;
use crate::internal::path::encode_path_param;
use crate::internal::request_options::RequestOptions;

// ─────────────────────────────────────────────────────────────────────────────
// Response types
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS VersionCreateResponse / VersionRetrieveResponse / VersionListResponse
///
/// A skill version object returned by the beta Skills API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionResponse {
    /// Unique identifier for the skill version.
    pub id: String,

    /// ISO 8601 timestamp of when the skill version was created.
    pub created_at: String,

    /// Description of the skill version (extracted from SKILL.md).
    pub description: String,

    /// Directory name of the skill version (top-level directory from upload).
    pub directory: String,

    /// Human-readable name of the skill version (extracted from SKILL.md).
    pub name: String,

    /// Identifier for the skill that this version belongs to.
    pub skill_id: String,

    /// Object type. For Skill Versions, this is always `"skill_version"`.
    #[serde(rename = "type")]
    pub type_name: String,

    /// Version identifier (Unix epoch timestamp).
    pub version: String,
}

impl Default for SkillVersionResponse {
    fn default() -> Self {
        Self {
            id: String::new(),
            created_at: String::new(),
            description: String::new(),
            directory: String::new(),
            name: String::new(),
            skill_id: String::new(),
            type_name: "skill_version".to_owned(),
            version: String::new(),
        }
    }
}

/// Maps to: TS VersionDeleteResponse
///
/// Confirmation of a skill version deletion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionDeleteResponse {
    /// Version identifier for the deleted skill version.
    pub id: String,

    /// Deleted object type. Always `"skill_version_deleted"`.
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for SkillVersionDeleteResponse {
    fn default() -> Self {
        Self {
            id: String::new(),
            type_name: "skill_version_deleted".to_owned(),
        }
    }
}

/// Maps to: TS VersionListResponsesPageCursor (PageCursor<VersionListResponse>)
///
/// A cursor-paginated list response for skill version listings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionListPage {
    /// The list of skill versions in this page.
    pub data: Vec<SkillVersionResponse>,

    /// Whether there are more pages after this one.
    #[serde(default)]
    pub has_more: bool,

    /// Cursor for the next page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page: Option<String>,
}

/// TS export-name compatibility alias for `VersionCreateResponse`.
pub type VersionCreateResponse = SkillVersionResponse;
/// TS export-name compatibility alias for `VersionRetrieveResponse`.
pub type VersionRetrieveResponse = SkillVersionResponse;
/// TS export-name compatibility alias for `VersionListResponse`.
pub type VersionListResponse = SkillVersionResponse;
/// TS export-name compatibility alias for `VersionDeleteResponse`.
pub type VersionDeleteResponse = SkillVersionDeleteResponse;
/// TS export-name compatibility alias for `VersionListResponsesPageCursor`.
pub type VersionListResponsesPageCursor = SkillVersionListPage;
/// TS export-name compatibility alias for `VersionCreateParams`.
pub type VersionCreateParams = SkillVersionCreateParams;
/// TS export-name compatibility alias for `VersionRetrieveParams`.
pub type VersionRetrieveParams = SkillVersionRetrieveParams;
/// TS export-name compatibility alias for `VersionListParams`.
pub type VersionListParams = SkillVersionListParams;
/// TS export-name compatibility alias for `VersionDeleteParams`.
pub type VersionDeleteParams = SkillVersionDeleteParams;

// ─────────────────────────────────────────────────────────────────────────────
// Request params
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS VersionCreateParams
///
/// Parameters for creating a skill version via multipart/form-data.
#[derive(Debug, Clone, Default)]
pub struct SkillVersionCreateParams {
    /// Body param: Files to upload for the skill version.
    pub files: Option<Vec<Uploadable>>,

    /// Optional beta version(s) to send in the `anthropic-beta` header.
    pub betas: Option<Vec<String>>,
}

/// Maps to: TS VersionRetrieveParams
///
/// Parameters for retrieving a skill version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionRetrieveParams {
    /// Unique identifier for the skill.
    pub skill_id: String,

    /// Optional beta version(s) to send in the `anthropic-beta` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub betas: Option<Vec<String>>,
}

/// Maps to: TS VersionListParams
///
/// Parameters for listing skill versions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillVersionListParams {
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

/// Maps to: TS VersionDeleteParams
///
/// Parameters for deleting a skill version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionDeleteParams {
    /// Unique identifier for the skill.
    pub skill_id: String,

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

/// Maps to: TS beta/skills Versions class
///
/// Resource for managing skill versions through the beta Skills API.
pub struct SkillVersions<'a> {
    client: &'a Anthropic,
}

/// TS export-name compatibility alias for the `Versions` resource class.
pub type Versions<'a> = SkillVersions<'a>;

impl<'a> SkillVersions<'a> {
    /// Create a new `SkillVersions` resource bound to the given client.
    pub fn new(client: &'a Anthropic) -> Self {
        Self { client }
    }

    /// Maps to: TS Versions.create() -- POST /v1/skills/{skill_id}/versions?beta=true
    ///
    /// Create a new skill version using multipart/form-data. Files are sent as
    /// repeated `files[]` fields, matching the TS SDK's form encoding.
    pub async fn create(
        &self,
        skill_id: &str,
        params: &SkillVersionCreateParams,
    ) -> Result<SkillVersionResponse, ApiError> {
        self.create_with_options(skill_id, params, None).await
    }

    /// Create a new skill version with per-request options.
    pub async fn create_with_options(
        &self,
        skill_id: &str,
        params: &SkillVersionCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<SkillVersionResponse, ApiError> {
        Ok(self
            .create_with_response_and_options(skill_id, params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.skills.versions.create(...).withResponse()`.
    pub async fn create_with_response(
        &self,
        skill_id: &str,
        params: &SkillVersionCreateParams,
    ) -> Result<ApiResponse<SkillVersionResponse>, ApiError> {
        self.create_with_response_and_options(skill_id, params, None)
            .await
    }

    /// Create a skill version returning parsed data plus raw response metadata/body.
    pub async fn create_with_response_and_options(
        &self,
        skill_id: &str,
        params: &SkillVersionCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<SkillVersionResponse>, ApiError> {
        let skill_id = encode_path_param(skill_id)?;
        let path = format!("/v1/skills/{skill_id}/versions?beta=true");
        let headers = build_skills_beta_header(params.betas.as_deref());

        self.client
            .post_multipart_with_response(
                &path,
                || {
                    let mut form = Form::new();
                    if let Some(files) = &params.files {
                        for file in files {
                            form = form.part("files[]", file.to_part(true)?);
                        }
                    }
                    Ok(form)
                },
                Some(&headers),
                options,
            )
            .await
    }

    /// Maps to: TS Versions.retrieve() -- GET /v1/skills/{skill_id}/versions/{version}?beta=true
    ///
    /// Retrieve a specific skill version.
    pub async fn retrieve(
        &self,
        version: &str,
        params: &SkillVersionRetrieveParams,
    ) -> Result<SkillVersionResponse, ApiError> {
        self.retrieve_with_options(version, params, None).await
    }

    /// Retrieve a specific skill version with per-request options.
    pub async fn retrieve_with_options(
        &self,
        version: &str,
        params: &SkillVersionRetrieveParams,
        options: Option<&RequestOptions>,
    ) -> Result<SkillVersionResponse, ApiError> {
        Ok(self
            .retrieve_with_response_and_options(version, params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.skills.versions.retrieve(...).withResponse()`.
    pub async fn retrieve_with_response(
        &self,
        version: &str,
        params: &SkillVersionRetrieveParams,
    ) -> Result<ApiResponse<SkillVersionResponse>, ApiError> {
        self.retrieve_with_response_and_options(version, params, None)
            .await
    }

    /// Retrieve a skill version returning parsed data plus raw response metadata/body.
    pub async fn retrieve_with_response_and_options(
        &self,
        version: &str,
        params: &SkillVersionRetrieveParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<SkillVersionResponse>, ApiError> {
        let skill_id = encode_path_param(&params.skill_id)?;
        let version = encode_path_param(version)?;
        let path = format!("/v1/skills/{skill_id}/versions/{version}?beta=true");
        let headers = build_skills_beta_header(params.betas.as_deref());

        self.client
            .get_with_response(&path, None, Some(&headers), options)
            .await
    }

    /// Maps to: TS Versions.list() -- GET /v1/skills/{skill_id}/versions?beta=true
    ///
    /// List skill versions. Returns a cursor-paginated [`SkillVersionListPage`].
    pub async fn list(
        &self,
        skill_id: &str,
        params: Option<&SkillVersionListParams>,
    ) -> Result<SkillVersionListPage, ApiError> {
        self.list_with_options(skill_id, params, None).await
    }

    /// List skill versions with per-request options.
    pub async fn list_with_options(
        &self,
        skill_id: &str,
        params: Option<&SkillVersionListParams>,
        options: Option<&RequestOptions>,
    ) -> Result<SkillVersionListPage, ApiError> {
        Ok(self
            .list_with_response_and_options(skill_id, params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.skills.versions.list(...).withResponse()`.
    pub async fn list_with_response(
        &self,
        skill_id: &str,
        params: Option<&SkillVersionListParams>,
    ) -> Result<ApiResponse<SkillVersionListPage>, ApiError> {
        self.list_with_response_and_options(skill_id, params, None)
            .await
    }

    /// List skill versions returning parsed data plus raw response metadata/body.
    pub async fn list_with_response_and_options(
        &self,
        skill_id: &str,
        params: Option<&SkillVersionListParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<SkillVersionListPage>, ApiError> {
        let skill_id = encode_path_param(skill_id)?;
        let path = format!("/v1/skills/{skill_id}/versions?beta=true");

        let mut query = HashMap::new();
        if let Some(p) = params {
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
            .get_with_response(&path, query_ref, Some(&headers), options)
            .await
    }

    /// Maps to: TS Versions.delete() -- DELETE /v1/skills/{skill_id}/versions/{version}?beta=true
    ///
    /// Delete a skill version.
    pub async fn delete(
        &self,
        version: &str,
        params: &SkillVersionDeleteParams,
    ) -> Result<SkillVersionDeleteResponse, ApiError> {
        self.delete_with_options(version, params, None).await
    }

    /// Delete a skill version with per-request options.
    pub async fn delete_with_options(
        &self,
        version: &str,
        params: &SkillVersionDeleteParams,
        options: Option<&RequestOptions>,
    ) -> Result<SkillVersionDeleteResponse, ApiError> {
        Ok(self
            .delete_with_response_and_options(version, params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.skills.versions.delete(...).withResponse()`.
    pub async fn delete_with_response(
        &self,
        version: &str,
        params: &SkillVersionDeleteParams,
    ) -> Result<ApiResponse<SkillVersionDeleteResponse>, ApiError> {
        self.delete_with_response_and_options(version, params, None)
            .await
    }

    /// Delete a skill version returning parsed data plus raw response metadata/body.
    pub async fn delete_with_response_and_options(
        &self,
        version: &str,
        params: &SkillVersionDeleteParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<SkillVersionDeleteResponse>, ApiError> {
        let skill_id = encode_path_param(&params.skill_id)?;
        let version = encode_path_param(version)?;
        let path = format!("/v1/skills/{skill_id}/versions/{version}?beta=true");
        let headers = build_skills_beta_header(params.betas.as_deref());

        self.client
            .delete_with_response(&path, Some(&headers), options)
            .await
    }
}
