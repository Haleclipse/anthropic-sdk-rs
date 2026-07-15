// Maps to: TS resources/beta/files.ts

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

/// Maps to: TS FileMetadata
///
/// Metadata about an uploaded file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// Unique object identifier.
    pub id: String,

    /// RFC 3339 datetime string representing when the file was created.
    pub created_at: String,

    /// Original filename of the uploaded file.
    pub filename: String,

    /// MIME type of the file.
    pub mime_type: String,

    /// Size of the file in bytes.
    pub size_bytes: i64,

    /// Object type. Always `"file"`.
    #[serde(rename = "type")]
    pub type_name: String,

    /// Whether the file can be downloaded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloadable: Option<bool>,
}

impl Default for FileMetadata {
    fn default() -> Self {
        Self {
            id: String::new(),
            created_at: String::new(),
            filename: String::new(),
            mime_type: String::new(),
            size_bytes: 0,
            type_name: "file".to_owned(),
            downloadable: None,
        }
    }
}

/// Maps to: TS DeletedFile
///
/// Confirmation of a file deletion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletedFile {
    /// ID of the deleted file.
    pub id: String,

    /// Deleted object type. Always `"file_deleted"`.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
}

impl Default for DeletedFile {
    fn default() -> Self {
        Self {
            id: String::new(),
            type_name: Some("file_deleted".to_owned()),
        }
    }
}

/// Maps to: TS FileMetadataPage (Page<FileMetadata>)
///
/// A paginated list response for file listings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadataPage {
    /// The list of files in this page.
    pub data: Vec<FileMetadata>,

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

/// Maps to: TS FileListParams
///
/// Parameters for listing files.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileListParams {
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

/// Maps to: TS FileDeleteParams
///
/// Parameters for deleting a file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileDeleteParams {
    /// Optional beta version(s) to send in the `anthropic-beta` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub betas: Option<Vec<String>>,
}

/// Maps to: TS FileDownloadParams
///
/// Parameters for downloading file content.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileDownloadParams {
    /// Optional beta version(s) to send in the `anthropic-beta` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub betas: Option<Vec<String>>,
}

/// Maps to: TS FileRetrieveMetadataParams
///
/// Parameters for retrieving file metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileRetrieveMetadataParams {
    /// Optional beta version(s) to send in the `anthropic-beta` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub betas: Option<Vec<String>>,
}

/// Maps to: TS FileUploadParams
///
/// Parameters for uploading a file via multipart/form-data.
#[derive(Debug, Clone)]
pub struct FileUploadParams {
    /// Body param: The file to upload.
    pub file: Uploadable,

    /// Optional beta version(s) to send in the `anthropic-beta` header.
    pub betas: Option<Vec<String>>,
}

impl FileUploadParams {
    /// Create upload parameters for a single file.
    pub fn new(file: Uploadable) -> Self {
        Self { file, betas: None }
    }

    /// Attach beta feature header values.
    pub fn with_betas(mut self, betas: Vec<String>) -> Self {
        self.betas = Some(betas);
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper
// ─────────────────────────────────────────────────────────────────────────────

/// Build the `anthropic-beta` header value, always including the
/// `files-api-2025-04-14` feature flag.
fn build_beta_header(betas: Option<&[String]>) -> HashMap<String, Option<String>> {
    let mut parts: Vec<String> = betas.map(|b| b.to_vec()).unwrap_or_default();
    parts.push("files-api-2025-04-14".to_owned());
    let mut h = HashMap::new();
    h.insert("anthropic-beta".to_owned(), Some(parts.join(",")));
    h
}

// ─────────────────────────────────────────────────────────────────────────────
// Resource
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS Files class
///
/// Resource for managing files through the beta Files API.
pub struct Files<'a> {
    client: &'a Anthropic,
}

impl<'a> Files<'a> {
    /// Create a new `Files` resource bound to the given client.
    pub fn new(client: &'a Anthropic) -> Self {
        Self { client }
    }

    /// Maps to: TS Files.retrieveMetadata() -- GET /v1/files/{file_id}
    ///
    /// Get file metadata.
    pub async fn retrieve_metadata(
        &self,
        file_id: &str,
        params: Option<&FileRetrieveMetadataParams>,
    ) -> Result<FileMetadata, ApiError> {
        self.retrieve_metadata_with_options(file_id, params, None)
            .await
    }

    /// TS-style camelCase alias for [`Files::retrieve_metadata`].
    #[allow(non_snake_case)]
    pub async fn retrieveMetadata(
        &self,
        file_id: &str,
        params: Option<&FileRetrieveMetadataParams>,
    ) -> Result<FileMetadata, ApiError> {
        self.retrieve_metadata(file_id, params).await
    }

    /// Get file metadata with per-request options.
    pub async fn retrieve_metadata_with_options(
        &self,
        file_id: &str,
        params: Option<&FileRetrieveMetadataParams>,
        options: Option<&RequestOptions>,
    ) -> Result<FileMetadata, ApiError> {
        let file_id = encode_path_param(file_id)?;
        let path = format!("/v1/files/{file_id}");
        let headers = build_beta_header(params.and_then(|p| p.betas.as_deref()));
        self.client
            .get_with_options(&path, None, Some(&headers), options)
            .await
    }

    /// Rust equivalent of TS `retrieveMetadata(...).withResponse()`.
    pub async fn retrieve_metadata_with_response(
        &self,
        file_id: &str,
        params: Option<&FileRetrieveMetadataParams>,
    ) -> Result<ApiResponse<FileMetadata>, ApiError> {
        self.retrieve_metadata_with_response_and_options(file_id, params, None)
            .await
    }

    /// Get file metadata returning parsed data plus raw response metadata/body.
    pub async fn retrieve_metadata_with_response_and_options(
        &self,
        file_id: &str,
        params: Option<&FileRetrieveMetadataParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<FileMetadata>, ApiError> {
        let file_id = encode_path_param(file_id)?;
        let path = format!("/v1/files/{file_id}");
        let headers = build_beta_header(params.and_then(|p| p.betas.as_deref()));
        self.client
            .get_with_response(&path, None, Some(&headers), options)
            .await
    }

    /// TS-style camelCase alias for [`Files::retrieve_metadata_with_options`].
    #[allow(non_snake_case)]
    pub async fn retrieveMetadataWithOptions(
        &self,
        file_id: &str,
        params: Option<&FileRetrieveMetadataParams>,
        options: Option<&RequestOptions>,
    ) -> Result<FileMetadata, ApiError> {
        self.retrieve_metadata_with_options(file_id, params, options)
            .await
    }

    /// Maps to: TS Files.list() -- GET /v1/files
    ///
    /// List files. Returns a paginated [`FileMetadataPage`].
    pub async fn list(
        &self,
        params: Option<&FileListParams>,
    ) -> Result<FileMetadataPage, ApiError> {
        self.list_with_options(params, None).await
    }

    /// List files with per-request options.
    pub async fn list_with_options(
        &self,
        params: Option<&FileListParams>,
        options: Option<&RequestOptions>,
    ) -> Result<FileMetadataPage, ApiError> {
        Ok(self
            .list_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `list(...).withResponse()`.
    pub async fn list_with_response(
        &self,
        params: Option<&FileListParams>,
    ) -> Result<ApiResponse<FileMetadataPage>, ApiError> {
        self.list_with_response_and_options(params, None).await
    }

    /// List files returning parsed data plus raw response metadata/body.
    pub async fn list_with_response_and_options(
        &self,
        params: Option<&FileListParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<FileMetadataPage>, ApiError> {
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

        let headers = build_beta_header(params.and_then(|p| p.betas.as_deref()));
        let query_ref = if query.is_empty() { None } else { Some(&query) };

        self.client
            .get_with_response("/v1/files", query_ref, Some(&headers), options)
            .await
    }

    /// Maps to: TS Files.delete() -- DELETE /v1/files/{file_id}
    ///
    /// Delete a file.
    pub async fn delete(
        &self,
        file_id: &str,
        params: Option<&FileDeleteParams>,
    ) -> Result<DeletedFile, ApiError> {
        self.delete_with_options(file_id, params, None).await
    }

    /// Delete a file with per-request options.
    pub async fn delete_with_options(
        &self,
        file_id: &str,
        params: Option<&FileDeleteParams>,
        options: Option<&RequestOptions>,
    ) -> Result<DeletedFile, ApiError> {
        Ok(self
            .delete_with_response_and_options(file_id, params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `delete(...).withResponse()`.
    pub async fn delete_with_response(
        &self,
        file_id: &str,
        params: Option<&FileDeleteParams>,
    ) -> Result<ApiResponse<DeletedFile>, ApiError> {
        self.delete_with_response_and_options(file_id, params, None)
            .await
    }

    /// Delete a file returning parsed data plus raw response metadata/body.
    pub async fn delete_with_response_and_options(
        &self,
        file_id: &str,
        params: Option<&FileDeleteParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<DeletedFile>, ApiError> {
        let file_id = encode_path_param(file_id)?;
        let path = format!("/v1/files/{file_id}");
        let headers = build_beta_header(params.and_then(|p| p.betas.as_deref()));

        self.client
            .request_with_response(
                reqwest::Method::DELETE,
                &path,
                None,
                Some(&headers),
                None,
                options,
            )
            .await
    }

    /// Maps to: TS Files.upload() -- POST /v1/files (multipart/form-data)
    ///
    /// Upload a file using the `file` multipart field.
    pub async fn upload(&self, params: &FileUploadParams) -> Result<FileMetadata, ApiError> {
        self.upload_with_options(params, None).await
    }

    /// Upload a file with per-request options.
    pub async fn upload_with_options(
        &self,
        params: &FileUploadParams,
        options: Option<&RequestOptions>,
    ) -> Result<FileMetadata, ApiError> {
        Ok(self
            .upload_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `upload(...).withResponse()`.
    pub async fn upload_with_response(
        &self,
        params: &FileUploadParams,
    ) -> Result<ApiResponse<FileMetadata>, ApiError> {
        self.upload_with_response_and_options(params, None).await
    }

    /// Upload a file returning parsed data plus raw response metadata/body.
    pub async fn upload_with_response_and_options(
        &self,
        params: &FileUploadParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<FileMetadata>, ApiError> {
        let mut headers = build_beta_header(params.betas.as_deref());
        if let Some(helper) = params.file.stainless_helper() {
            headers.insert("x-stainless-helper".to_owned(), Some(helper.to_owned()));
        }

        self.client
            .post_multipart_with_response(
                "/v1/files",
                || {
                    let part = params.file.to_part(true)?;
                    Ok(Form::new().part("file", part))
                },
                Some(&headers),
                options,
            )
            .await
    }

    /// Maps to: TS Files.download() -- GET /v1/files/{file_id}/content
    ///
    /// Download a file's raw binary content.
    pub async fn download(
        &self,
        file_id: &str,
        params: Option<&FileDownloadParams>,
    ) -> Result<Vec<u8>, ApiError> {
        self.download_with_options(file_id, params, None).await
    }

    /// Download a file's raw binary content with per-request options.
    pub async fn download_with_options(
        &self,
        file_id: &str,
        params: Option<&FileDownloadParams>,
        options: Option<&RequestOptions>,
    ) -> Result<Vec<u8>, ApiError> {
        Ok(self
            .download_with_response_and_options(file_id, params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `download(...).withResponse()`.
    pub async fn download_with_response(
        &self,
        file_id: &str,
        params: Option<&FileDownloadParams>,
    ) -> Result<ApiResponse<Vec<u8>>, ApiError> {
        self.download_with_response_and_options(file_id, params, None)
            .await
    }

    /// Download binary content returning bytes plus raw response metadata/body.
    pub async fn download_with_response_and_options(
        &self,
        file_id: &str,
        params: Option<&FileDownloadParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<Vec<u8>>, ApiError> {
        let file_id = encode_path_param(file_id)?;
        let path = format!("/v1/files/{file_id}/content");
        let mut headers = build_beta_header(params.and_then(|p| p.betas.as_deref()));
        headers.insert("Accept".to_owned(), Some("application/binary".to_owned()));

        self.client
            .get_binary_with_response(&path, None, Some(&headers), options)
            .await
    }
}
