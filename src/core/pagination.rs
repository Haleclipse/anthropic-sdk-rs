// Maps to: TS core/pagination.ts

use std::future::Future;

use serde::{Deserialize, Serialize};

use crate::core::error::ApiError;

/// Maps to: TS `PageParams`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_id: Option<String>,
}

/// Maps to: TS `TokenPageParams`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenPageParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

/// Maps to: TS `PageCursorParams`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageCursorParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
}

/// An offset-based page of results using `first_id` / `last_id` cursors.
///
/// Maps to: TS `Page<Item>` class in core/pagination.ts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    /// The items on this page.
    #[serde(default)]
    pub data: Vec<T>,
    /// Whether more pages exist beyond this one.
    #[serde(default)]
    pub has_more: bool,
    /// The ID of the first item on this page (used for backward pagination).
    pub first_id: Option<String>,
    /// The ID of the last item on this page (used for forward pagination).
    pub last_id: Option<String>,
}

impl<T> IntoIterator for Page<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.into_iter()
    }
}

impl<T> Page<T> {
    /// Returns the items on this page.
    ///
    /// Maps to: TS `Page.getPaginatedItems()`.
    pub fn get_paginated_items(&self) -> &[T] {
        &self.data
    }

    /// TS-style camelCase alias for [`Page::get_paginated_items`].
    #[allow(non_snake_case)]
    pub fn getPaginatedItems(&self) -> &[T] {
        self.get_paginated_items()
    }

    /// Returns `true` if there are more pages available.
    ///
    /// Maps to: TS `Page.hasNextPage()`
    pub fn has_next_page(&self) -> bool {
        self.has_more
            && !self.data.is_empty()
            && (nonempty_cursor(&self.last_id).is_some()
                || nonempty_cursor(&self.first_id).is_some())
    }

    /// TS-style camelCase alias for [`Page::has_next_page`].
    #[allow(non_snake_case)]
    pub fn hasNextPage(&self) -> bool {
        self.has_next_page()
    }

    /// Calculates params for the next page using TS `Page.nextPageRequestOptions()`
    /// cursor semantics. If `current.before_id` is set, pagination is reverse
    /// and the next request uses `first_id`; otherwise it uses `last_id` as
    /// `after_id`. Other params such as `limit` are preserved.
    pub fn next_page_params(&self, current: Option<&PageParams>) -> Option<PageParams> {
        let mut params = current.cloned().unwrap_or_default();
        if nonempty_cursor(&params.before_id).is_some() {
            params.before_id = Some(nonempty_cursor(&self.first_id)?.to_owned());
        } else {
            params.after_id = Some(nonempty_cursor(&self.last_id)?.to_owned());
        }
        Some(params)
    }

    /// TS-style camelCase alias for [`Page::next_page_params`].
    #[allow(non_snake_case)]
    pub fn nextPageParams(&self, current: Option<&PageParams>) -> Option<PageParams> {
        self.next_page_params(current)
    }
}

/// Maps to: TS `PageResponse<Item>`.
pub type PageResponse<T> = Page<T>;

/// A cursor-based page of results using an opaque `next_page` token.
///
/// Maps to: TS `TokenPage<Item>` / `PageCursor<Item>` classes in core/pagination.ts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPage<T> {
    /// The items on this page.
    #[serde(default)]
    pub data: Vec<T>,
    /// Whether more pages exist beyond this one.
    #[serde(default)]
    pub has_more: bool,
    /// Opaque token for fetching the next page.
    pub next_page: Option<String>,
}

impl<T> IntoIterator for CursorPage<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.into_iter()
    }
}

impl<T> CursorPage<T> {
    /// Returns the items on this page.
    ///
    /// Maps to: TS `TokenPage.getPaginatedItems()` /
    /// `PageCursor.getPaginatedItems()`.
    pub fn get_paginated_items(&self) -> &[T] {
        &self.data
    }

    /// TS-style camelCase alias for [`CursorPage::get_paginated_items`].
    #[allow(non_snake_case)]
    pub fn getPaginatedItems(&self) -> &[T] {
        self.get_paginated_items()
    }

    /// Returns `true` if there are more pages available.
    ///
    /// Maps to: TS `TokenPage.hasNextPage()` / `PageCursor.hasNextPage()`
    pub fn has_next_page(&self) -> bool {
        self.has_more && !self.data.is_empty() && nonempty_cursor(&self.next_page).is_some()
    }

    /// TS-style camelCase alias for [`CursorPage::has_next_page`].
    #[allow(non_snake_case)]
    pub fn hasNextPage(&self) -> bool {
        self.has_next_page()
    }

    /// Calculates params for the next token page using TS
    /// `TokenPage.nextPageRequestOptions()` semantics (`page_token`).
    pub fn next_token_page_params(
        &self,
        current: Option<&TokenPageParams>,
    ) -> Option<TokenPageParams> {
        let mut params = current.cloned().unwrap_or_default();
        params.page_token = Some(nonempty_cursor(&self.next_page)?.to_owned());
        Some(params)
    }

    /// TS-style camelCase alias for [`CursorPage::next_token_page_params`].
    #[allow(non_snake_case)]
    pub fn nextTokenPageParams(
        &self,
        current: Option<&TokenPageParams>,
    ) -> Option<TokenPageParams> {
        self.next_token_page_params(current)
    }

    /// Calculates params for the next cursor page using TS
    /// `PageCursor.nextPageRequestOptions()` semantics (`page`).
    pub fn next_page_cursor_params(
        &self,
        current: Option<&PageCursorParams>,
    ) -> Option<PageCursorParams> {
        let mut params = current.cloned().unwrap_or_default();
        params.page = Some(nonempty_cursor(&self.next_page)?.to_owned());
        Some(params)
    }

    /// TS-style camelCase alias for [`CursorPage::next_page_cursor_params`].
    #[allow(non_snake_case)]
    pub fn nextPageCursorParams(
        &self,
        current: Option<&PageCursorParams>,
    ) -> Option<PageCursorParams> {
        self.next_page_cursor_params(current)
    }
}

fn nonempty_cursor(cursor: &Option<String>) -> Option<&str> {
    cursor.as_deref().filter(|value| !value.is_empty())
}

/// Maps to: TS `TokenPage<Item>`.
pub type TokenPage<T> = CursorPage<T>;
/// Maps to: TS `TokenPageResponse<Item>`.
pub type TokenPageResponse<T> = CursorPage<T>;
/// Maps to: TS `PageCursor<Item>`.
pub type PageCursor<T> = CursorPage<T>;
/// Maps to: TS `PageCursorResponse<Item>`.
pub type PageCursorResponse<T> = CursorPage<T>;

// ---------------------------------------------------------------------------
// Go-native auto-pagination helpers
// ---------------------------------------------------------------------------

/// Collect all items from offset/id-cursor pages.
///
/// This is a Rust-native counterpart to the Go SDK's `ListAutoPaging()` helper:
/// the caller supplies a page-fetching async closure, while this helper keeps
/// forwarding the next-page params until `has_next_page()` becomes false.
pub async fn collect_all_pages<T, F, Fut>(
    initial_params: PageParams,
    mut fetch_page: F,
) -> Result<Vec<T>, ApiError>
where
    F: FnMut(PageParams) -> Fut,
    Fut: Future<Output = Result<Page<T>, ApiError>>,
{
    let mut params = initial_params;
    let mut items = Vec::new();

    loop {
        let page = fetch_page(params.clone()).await?;
        let next = if page.has_next_page() {
            page.next_page_params(Some(&params))
        } else {
            None
        };
        items.extend(page.data);

        let Some(next) = next else {
            break;
        };
        params = next;
    }

    Ok(items)
}

/// CamelCase alias for [`collect_all_pages`].
#[allow(non_snake_case)]
pub async fn collectAllPages<T, F, Fut>(
    initial_params: PageParams,
    fetch_page: F,
) -> Result<Vec<T>, ApiError>
where
    F: FnMut(PageParams) -> Fut,
    Fut: Future<Output = Result<Page<T>, ApiError>>,
{
    collect_all_pages(initial_params, fetch_page).await
}

/// Collect all items from token pages using `page_token`.
pub async fn collect_all_token_pages<T, F, Fut>(
    initial_params: TokenPageParams,
    mut fetch_page: F,
) -> Result<Vec<T>, ApiError>
where
    F: FnMut(TokenPageParams) -> Fut,
    Fut: Future<Output = Result<TokenPage<T>, ApiError>>,
{
    let mut params = initial_params;
    let mut items = Vec::new();

    loop {
        let page = fetch_page(params.clone()).await?;
        let next = if page.has_next_page() {
            page.next_token_page_params(Some(&params))
        } else {
            None
        };
        items.extend(page.data);

        let Some(next) = next else {
            break;
        };
        params = next;
    }

    Ok(items)
}

/// CamelCase alias for [`collect_all_token_pages`].
#[allow(non_snake_case)]
pub async fn collectAllTokenPages<T, F, Fut>(
    initial_params: TokenPageParams,
    fetch_page: F,
) -> Result<Vec<T>, ApiError>
where
    F: FnMut(TokenPageParams) -> Fut,
    Fut: Future<Output = Result<TokenPage<T>, ApiError>>,
{
    collect_all_token_pages(initial_params, fetch_page).await
}

/// Collect all items from page-cursor pages using `page`.
pub async fn collect_all_page_cursor_pages<T, F, Fut>(
    initial_params: PageCursorParams,
    mut fetch_page: F,
) -> Result<Vec<T>, ApiError>
where
    F: FnMut(PageCursorParams) -> Fut,
    Fut: Future<Output = Result<PageCursor<T>, ApiError>>,
{
    let mut params = initial_params;
    let mut items = Vec::new();

    loop {
        let page = fetch_page(params.clone()).await?;
        let next = if page.has_next_page() {
            page.next_page_cursor_params(Some(&params))
        } else {
            None
        };
        items.extend(page.data);

        let Some(next) = next else {
            break;
        };
        params = next;
    }

    Ok(items)
}

/// CamelCase alias for [`collect_all_page_cursor_pages`].
#[allow(non_snake_case)]
pub async fn collectAllPageCursorPages<T, F, Fut>(
    initial_params: PageCursorParams,
    fetch_page: F,
) -> Result<Vec<T>, ApiError>
where
    F: FnMut(PageCursorParams) -> Fut,
    Fut: Future<Output = Result<PageCursor<T>, ApiError>>,
{
    collect_all_page_cursor_pages(initial_params, fetch_page).await
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_into_iter() {
        let page = Page {
            data: vec![1, 2, 3],
            has_more: true,
            first_id: Some("id_1".to_string()),
            last_id: Some("id_3".to_string()),
        };
        assert!(page.has_next_page());
        let collected: Vec<i32> = page.into_iter().collect();
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    fn page_no_more() {
        let page: Page<String> = Page {
            data: vec![],
            has_more: false,
            first_id: None,
            last_id: None,
        };
        assert!(!page.has_next_page());
        let collected: Vec<String> = page.into_iter().collect();
        assert!(collected.is_empty());
    }

    #[test]
    fn page_has_next_page_requires_items_and_cursor_like_ts() {
        let empty: Page<i32> = Page {
            data: vec![],
            has_more: true,
            first_id: Some("id_1".to_owned()),
            last_id: Some("id_1".to_owned()),
        };
        assert!(!empty.has_next_page());

        let missing_cursor = Page {
            data: vec![1],
            has_more: true,
            first_id: None,
            last_id: None,
        };
        assert!(!missing_cursor.has_next_page());

        let empty_cursors = Page {
            data: vec![1],
            has_more: true,
            first_id: Some(String::new()),
            last_id: Some(String::new()),
        };
        assert!(!empty_cursors.has_next_page());
        assert_eq!(empty_cursors.next_page_params(None), None);
    }

    #[test]
    fn cursor_page_into_iter() {
        let page = CursorPage {
            data: vec!["a".to_string(), "b".to_string()],
            has_more: true,
            next_page: Some("token_abc".to_string()),
        };
        assert!(page.has_next_page());
        let collected: Vec<String> = page.into_iter().collect();
        assert_eq!(collected, vec!["a", "b"]);
    }

    #[test]
    fn cursor_page_no_more() {
        let page: CursorPage<u64> = CursorPage {
            data: vec![42],
            has_more: false,
            next_page: None,
        };
        assert!(!page.has_next_page());
        let collected: Vec<u64> = page.into_iter().collect();
        assert_eq!(collected, vec![42]);
    }

    #[test]
    fn cursor_page_has_next_page_requires_items_and_cursor_like_ts() {
        let empty: CursorPage<i32> = CursorPage {
            data: vec![],
            has_more: true,
            next_page: Some("next".to_owned()),
        };
        assert!(!empty.has_next_page());

        let missing_cursor = CursorPage {
            data: vec![1],
            has_more: true,
            next_page: None,
        };
        assert!(!missing_cursor.has_next_page());

        let empty_cursor = CursorPage {
            data: vec![1],
            has_more: true,
            next_page: Some(String::new()),
        };
        assert!(!empty_cursor.has_next_page());
        assert_eq!(empty_cursor.next_token_page_params(None), None);
        assert_eq!(empty_cursor.next_page_cursor_params(None), None);
    }

    #[test]
    fn page_next_page_params_match_ts_cursor_semantics_and_aliases() {
        let page = Page {
            data: vec![1, 2],
            has_more: true,
            first_id: Some("id_1".to_owned()),
            last_id: Some("id_2".to_owned()),
        };

        assert_eq!(page.get_paginated_items(), &[1, 2]);
        assert_eq!(page.getPaginatedItems(), &[1, 2]);
        assert!(page.hasNextPage());

        let forward = page.next_page_params(Some(&PageParams {
            limit: Some(10),
            before_id: None,
            after_id: Some("old".to_owned()),
        }));
        assert_eq!(
            forward,
            Some(PageParams {
                limit: Some(10),
                before_id: None,
                after_id: Some("id_2".to_owned()),
            })
        );

        let reverse = page.nextPageParams(Some(&PageParams {
            limit: Some(10),
            before_id: Some("old".to_owned()),
            after_id: None,
        }));
        assert_eq!(
            reverse,
            Some(PageParams {
                limit: Some(10),
                before_id: Some("id_1".to_owned()),
                after_id: None,
            })
        );
    }

    #[test]
    fn cursor_page_next_page_params_match_ts_token_and_page_cursor_semantics() {
        let page = CursorPage {
            data: vec!["a".to_owned()],
            has_more: true,
            next_page: Some("next_token".to_owned()),
        };

        assert_eq!(page.get_paginated_items(), ["a".to_owned()]);
        assert_eq!(page.getPaginatedItems(), ["a".to_owned()]);
        assert!(page.hasNextPage());

        assert_eq!(
            page.next_token_page_params(Some(&TokenPageParams {
                limit: Some(3),
                page_token: Some("old".to_owned()),
            })),
            Some(TokenPageParams {
                limit: Some(3),
                page_token: Some("next_token".to_owned()),
            })
        );
        assert_eq!(
            page.nextTokenPageParams(None),
            Some(TokenPageParams {
                limit: None,
                page_token: Some("next_token".to_owned()),
            })
        );
        assert_eq!(
            page.next_page_cursor_params(Some(&PageCursorParams {
                limit: Some(4),
                page: Some("old".to_owned()),
            })),
            Some(PageCursorParams {
                limit: Some(4),
                page: Some("next_token".to_owned()),
            })
        );
        assert_eq!(
            page.nextPageCursorParams(None),
            Some(PageCursorParams {
                limit: None,
                page: Some("next_token".to_owned()),
            })
        );
    }

    #[test]
    fn page_serialize_roundtrip() {
        let page = Page {
            data: vec![10, 20],
            has_more: true,
            first_id: Some("f".to_string()),
            last_id: Some("l".to_string()),
        };
        let json = serde_json::to_string(&page).expect("serialize");
        let deserialized: Page<i32> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.data, vec![10, 20]);
        assert!(deserialized.has_more);
        assert_eq!(deserialized.first_id.as_deref(), Some("f"));
        assert_eq!(deserialized.last_id.as_deref(), Some("l"));
    }

    #[test]
    fn cursor_page_serialize_roundtrip() {
        let page = CursorPage {
            data: vec!["x".to_string()],
            has_more: false,
            next_page: None,
        };
        let json = serde_json::to_string(&page).expect("serialize");
        let deserialized: CursorPage<String> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.data, vec!["x"]);
        assert!(!deserialized.has_more);
        assert!(deserialized.next_page.is_none());
    }

    #[test]
    fn missing_page_response_fields_default_like_ts_constructors() {
        let page: Page<i32> = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(page.data.is_empty());
        assert!(!page.has_more);
        assert!(page.first_id.is_none());
        assert!(page.last_id.is_none());

        let cursor: CursorPage<i32> = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(cursor.data.is_empty());
        assert!(!cursor.has_more);
        assert!(cursor.next_page.is_none());
    }

    #[test]
    fn ts_pagination_aliases_and_params_are_available() {
        let params = PageParams {
            limit: Some(20),
            before_id: None,
            after_id: Some("id_1".to_owned()),
        };
        let value = serde_json::to_value(params).unwrap();
        assert_eq!(value, serde_json::json!({"limit":20,"after_id":"id_1"}));

        let token_params = TokenPageParams {
            limit: Some(10),
            page_token: Some("tok".to_owned()),
        };
        assert_eq!(
            serde_json::to_value(token_params).unwrap()["page_token"],
            "tok"
        );

        let cursor_params = PageCursorParams {
            limit: None,
            page: Some("page_2".to_owned()),
        };
        assert_eq!(
            serde_json::to_value(cursor_params).unwrap()["page"],
            "page_2"
        );

        let token_page: TokenPage<i32> = CursorPage {
            data: vec![1],
            has_more: true,
            next_page: Some("next".to_owned()),
        };
        assert!(token_page.has_next_page());

        let page_cursor: PageCursor<i32> = token_page;
        assert!(page_cursor.has_next_page());
    }

    #[tokio::test]
    async fn collect_all_pages_walks_offset_cursor_pages_like_go_auto_paging() {
        let items = collect_all_pages(
            PageParams {
                limit: Some(2),
                ..Default::default()
            },
            |params| async move {
                Ok(match params.after_id.as_deref() {
                    None => Page {
                        data: vec![1, 2],
                        has_more: true,
                        first_id: Some("1".to_owned()),
                        last_id: Some("2".to_owned()),
                    },
                    Some("2") => Page {
                        data: vec![3],
                        has_more: false,
                        first_id: Some("3".to_owned()),
                        last_id: Some("3".to_owned()),
                    },
                    other => panic!("unexpected after_id: {other:?}"),
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(items, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn collect_all_token_pages_walks_next_page_tokens() {
        let items = collect_all_token_pages(TokenPageParams::default(), |params| async move {
            Ok(match params.page_token.as_deref() {
                None => CursorPage {
                    data: vec!["a"],
                    has_more: true,
                    next_page: Some("next".to_owned()),
                },
                Some("next") => CursorPage {
                    data: vec!["b"],
                    has_more: false,
                    next_page: None,
                },
                other => panic!("unexpected page_token: {other:?}"),
            })
        })
        .await
        .unwrap();

        assert_eq!(items, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn collect_all_page_cursor_pages_walks_page_params() {
        let items = collectAllPageCursorPages(PageCursorParams::default(), |params| async move {
            Ok(match params.page.as_deref() {
                None => CursorPage {
                    data: vec![10],
                    has_more: true,
                    next_page: Some("page-2".to_owned()),
                },
                Some("page-2") => CursorPage {
                    data: vec![20],
                    has_more: false,
                    next_page: None,
                },
                other => panic!("unexpected page: {other:?}"),
            })
        })
        .await
        .unwrap();

        assert_eq!(items, vec![10, 20]);
    }
}
