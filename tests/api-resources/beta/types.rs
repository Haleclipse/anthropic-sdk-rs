// Mirrors TS resources/beta/index.ts and beta/beta.ts exported-name surface.

use std::any::TypeId;

use anthropic_sdk::resources::beta::{
    BetaAPIError, BetaAuthenticationError, BetaBillingError, BetaError, BetaGatewayTimeoutError,
    BetaInvalidRequestError, BetaNotFoundError, BetaOverloadedError, BetaPermissionError,
    BetaRateLimitError, FileMetadata, Files, SkillCreateResponse, SkillListResponse,
    SkillListResponsesPageCursor, SkillResponse, SkillRetrieveResponse, SkillVersions,
    VersionCreateParams, VersionCreateResponse, VersionDeleteParams, VersionDeleteResponse,
    VersionListParams, VersionListResponse, VersionListResponsesPageCursor, VersionRetrieveParams,
    VersionRetrieveResponse, Versions,
};

fn assert_same_type<T: 'static, U: 'static>() {
    assert_eq!(TypeId::of::<T>(), TypeId::of::<U>());
}

#[test]
fn beta_index_reexports_file_skill_and_version_names_like_ts() {
    // Re-export smoke checks: these names are imported from resources::beta,
    // matching TS resources/beta/index.ts rather than nested Rust modules only.
    assert_same_type::<SkillCreateResponse, SkillResponse>();
    assert_same_type::<SkillRetrieveResponse, SkillResponse>();
    assert_same_type::<SkillListResponse, SkillResponse>();
    assert_same_type::<VersionCreateResponse, VersionRetrieveResponse>();
    assert_same_type::<VersionCreateResponse, VersionListResponse>();
    assert_same_type::<
        VersionDeleteResponse,
        anthropic_sdk::resources::beta::SkillVersionDeleteResponse,
    >();
    assert_same_type::<
        VersionListResponsesPageCursor,
        anthropic_sdk::resources::beta::SkillVersionListPage,
    >();
    assert_same_type::<SkillListResponsesPageCursor, anthropic_sdk::resources::beta::SkillListPage>(
    );
    assert_same_type::<VersionCreateParams, anthropic_sdk::resources::beta::SkillVersionCreateParams>(
    );
    assert_same_type::<
        VersionRetrieveParams,
        anthropic_sdk::resources::beta::SkillVersionRetrieveParams,
    >();
    assert_same_type::<VersionListParams, anthropic_sdk::resources::beta::SkillVersionListParams>();
    assert_same_type::<VersionDeleteParams, anthropic_sdk::resources::beta::SkillVersionDeleteParams>(
    );
    assert_same_type::<Versions<'static>, SkillVersions<'static>>();

    // Resource/type names re-exported from nested beta modules.
    assert_eq!(
        std::mem::size_of::<Files<'static>>(),
        std::mem::size_of::<Files<'static>>()
    );
    assert_eq!(
        std::mem::size_of::<FileMetadata>(),
        std::mem::size_of::<FileMetadata>()
    );
}

#[test]
fn beta_error_export_aliases_match_ts_names() {
    assert_same_type::<BetaAPIError, BetaError>();
    assert_same_type::<BetaAuthenticationError, BetaError>();
    assert_same_type::<BetaBillingError, BetaError>();
    assert_same_type::<BetaGatewayTimeoutError, BetaError>();
    assert_same_type::<BetaInvalidRequestError, BetaError>();
    assert_same_type::<BetaNotFoundError, BetaError>();
    assert_same_type::<BetaOverloadedError, BetaError>();
    assert_same_type::<BetaPermissionError, BetaError>();
    assert_same_type::<BetaRateLimitError, BetaError>();
}
