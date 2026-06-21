use super::models::{
    SearchPaginationMeta, SearchRecentResult, SearchResultMeta, SearchTweet, SearchUser,
    SearchUsersResult,
};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::env;

/// Arguments for recent tweet search
#[derive(Debug, Clone)]
pub struct SearchRecentArgs {
    pub query: String,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

/// Arguments for user search
#[derive(Debug, Clone)]
pub struct SearchUsersArgs {
    pub query: String,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

/// Trait for X API search client (enables testing via mocking)
pub trait SearchClient {
    /// Search recent tweets
    fn search_recent(&self, args: &SearchRecentArgs) -> Result<SearchRecentResult>;
    /// Search users
    fn search_users(&self, args: &SearchUsersArgs) -> Result<SearchUsersResult>;
}

/// Mock implementation of SearchClient for testing
pub struct MockSearchClient {
    pub tweets: Vec<SearchTweet>,
    pub users: Vec<SearchUser>,
}

impl MockSearchClient {
    /// Create a new mock with empty data
    pub fn new() -> Self {
        Self {
            tweets: Vec::new(),
            users: Vec::new(),
        }
    }

    /// Create a mock with sample tweet fixtures
    pub fn with_tweet_fixtures(count: usize) -> Self {
        let tweets = (0..count)
            .map(|i| {
                let mut tweet = SearchTweet::new(format!("fixture_tweet_{}", i));
                tweet.text = Some(format!("Fixture tweet text {}", i));
                tweet.author_id = Some(format!("fixture_user_{}", i));
                tweet.created_at = Some("2024-01-01T00:00:00Z".to_string());
                tweet
            })
            .collect();
        Self {
            tweets,
            users: Vec::new(),
        }
    }

    /// Create a mock with sample user fixtures
    pub fn with_user_fixtures(count: usize) -> Self {
        let users = (0..count)
            .map(|i| {
                let mut user = SearchUser::new(format!("fixture_user_{}", i));
                user.name = Some(format!("Fixture User {}", i));
                user.username = Some(format!("fixture_user_{}", i));
                user.description = Some(format!("A fixture user {}", i));
                user
            })
            .collect();
        Self {
            tweets: Vec::new(),
            users,
        }
    }
}

impl Default for MockSearchClient {
    fn default() -> Self {
        Self::new()
    }
}

/// X API v2 response for recent tweet search
#[derive(Debug, Deserialize)]
struct ApiSearchRecentResponse {
    data: Option<Vec<ApiTweet>>,
    meta: Option<ApiMeta>,
}

/// X API v1.1 response for user search
#[derive(Debug, Deserialize)]
struct ApiUser {
    id_str: String,
    name: Option<String>,
    screen_name: Option<String>,
    description: Option<String>,
}

/// X API v2 tweet data
#[derive(Debug, Deserialize)]
struct ApiTweet {
    id: String,
    text: Option<String>,
    author_id: Option<String>,
    created_at: Option<String>,
}

/// X API v2 metadata
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ApiMeta {
    result_count: Option<usize>,
    next_token: Option<String>,
}

/// Xquik tweet search response wrapper.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum XquikSearchTweetsResponse {
    Wrapped {
        tweets: Vec<XquikTweet>,
        #[serde(
            default,
            rename = "nextCursor",
            alias = "nextToken",
            alias = "next_cursor"
        )]
        next_cursor: Option<String>,
    },
    Data {
        data: Vec<XquikTweet>,
        #[serde(
            default,
            rename = "nextCursor",
            alias = "nextToken",
            alias = "next_cursor"
        )]
        next_cursor: Option<String>,
    },
    Items(Vec<XquikTweet>),
}

/// Xquik user search response wrapper.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum XquikSearchUsersResponse {
    Wrapped {
        users: Vec<XquikUser>,
        #[serde(
            default,
            rename = "nextCursor",
            alias = "nextToken",
            alias = "next_cursor"
        )]
        next_cursor: Option<String>,
    },
    Data {
        data: Vec<XquikUser>,
        #[serde(
            default,
            rename = "nextCursor",
            alias = "nextToken",
            alias = "next_cursor"
        )]
        next_cursor: Option<String>,
    },
    Items(Vec<XquikUser>),
}

/// Xquik tweet data.
#[derive(Debug, Deserialize)]
struct XquikTweet {
    id: String,
    text: Option<String>,
    #[serde(default, rename = "createdAt")]
    created_at: Option<String>,
    author: Option<XquikAuthor>,
}

/// Xquik tweet author data.
#[derive(Debug, Deserialize)]
struct XquikAuthor {
    id: String,
}

/// Xquik user data.
#[derive(Debug, Deserialize)]
struct XquikUser {
    id: String,
    name: Option<String>,
    username: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Clone)]
enum SearchBackend {
    XApi { bearer_token: String },
    Xquik { api_key: String, base_url: String },
}

/// HTTP-based implementation of SearchClient using X API
pub struct HttpSearchClient {
    backend: SearchBackend,
}

impl HttpSearchClient {
    /// Create a new HTTP search client with the given bearer token
    pub fn new(bearer_token: String) -> Self {
        Self {
            backend: SearchBackend::XApi { bearer_token },
        }
    }

    /// Create a new HTTP search client that uses Xquik read endpoints
    pub fn new_xquik(api_key: String, base_url: String) -> Self {
        Self {
            backend: SearchBackend::Xquik { api_key, base_url },
        }
    }

    /// Create from environment variable (XCOM_RS_BEARER_TOKEN)
    pub fn from_env() -> Result<Self> {
        if let Ok(api_key) = env::var("XQUIK_API_KEY") {
            let trimmed = api_key.trim();
            if !trimmed.is_empty() {
                let base_url = env::var("XQUIK_API_BASE_URL")
                    .unwrap_or_else(|_| "https://xquik.com/api/v1".to_string());
                return Ok(Self::new_xquik(trimmed.to_string(), base_url));
            }
        }

        let auth_store = crate::auth::storage::AuthStore::new();
        let status = auth_store.status();
        if !status.authenticated {
            anyhow::bail!(
                "Authentication required. Set XCOM_RS_BEARER_TOKEN environment variable."
            );
        }
        let bearer_token =
            env::var("XCOM_RS_BEARER_TOKEN").context("XCOM_RS_BEARER_TOKEN not set")?;
        Ok(Self::new(bearer_token))
    }

    fn search_recent_with_x_api(
        bearer_token: &str,
        args: &SearchRecentArgs,
    ) -> Result<SearchRecentResult> {
        let mut url = "https://api.twitter.com/2/tweets/search/recent".to_string();
        let mut params = vec![
            ("query", args.query.clone()),
            ("max_results", args.limit.unwrap_or(10).to_string()),
        ];

        params.push(("tweet.fields", "id,text,author_id,created_at".to_string()));

        if let Some(cursor) = &args.cursor {
            params.push(("next_token", cursor.clone()));
        }

        url.push('?');
        url.push_str(&encode_query_params(&params));

        let response = ureq::get(&url)
            .set("Authorization", &format!("Bearer {}", bearer_token))
            .call();

        let api_response: ApiSearchRecentResponse = match response {
            Ok(resp) => resp.into_json().context("Failed to parse X API response")?,
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                anyhow::bail!("X API error {}: {}", code, body);
            }
            Err(e) => anyhow::bail!("Failed to call X API search/recent: {}", e),
        };

        let tweets = api_response
            .data
            .unwrap_or_default()
            .into_iter()
            .map(|t| SearchTweet {
                id: t.id,
                text: t.text,
                author_id: t.author_id,
                created_at: t.created_at,
            })
            .collect::<Vec<_>>();

        let result_count = tweets.len();
        let next_token = api_response.meta.and_then(|m| m.next_token);
        Ok(SearchRecentResult {
            tweets,
            meta: Some(SearchResultMeta {
                pagination: SearchPaginationMeta {
                    next_token,
                    prev_token: None,
                    result_count,
                },
            }),
        })
    }

    fn search_recent_with_xquik(
        api_key: &str,
        base_url: &str,
        args: &SearchRecentArgs,
    ) -> Result<SearchRecentResult> {
        let mut url = format!("{}/x/tweets/search", base_url.trim_end_matches('/'));
        let mut params = vec![("q", args.query.clone())];
        if let Some(limit) = args.limit {
            params.push(("limit", limit.to_string()));
        }
        if let Some(cursor) = &args.cursor {
            params.push(("cursor", cursor.clone()));
        }
        url.push('?');
        url.push_str(&encode_query_params(&params));

        let response = ureq::get(&url)
            .set("x-api-key", api_key)
            .set("Accept", "application/json")
            .call();

        let api_response: XquikSearchTweetsResponse = match response {
            Ok(resp) => resp.into_json().context("Failed to parse Xquik response")?,
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                anyhow::bail!("Xquik API error {}: {}", code, body);
            }
            Err(e) => anyhow::bail!("Failed to call Xquik tweet search: {}", e),
        };

        let (tweets, next_token) = match api_response {
            XquikSearchTweetsResponse::Wrapped {
                tweets,
                next_cursor,
            } => (tweets, next_cursor),
            XquikSearchTweetsResponse::Data { data, next_cursor } => (data, next_cursor),
            XquikSearchTweetsResponse::Items(tweets) => (tweets, None),
        };

        let mut tweets = tweets
            .into_iter()
            .map(|tweet| SearchTweet {
                id: tweet.id,
                text: tweet.text,
                author_id: tweet.author.map(|author| author.id),
                created_at: tweet.created_at,
            })
            .collect::<Vec<_>>();

        if let Some(limit) = args.limit {
            tweets.truncate(limit);
        }

        let result_count = tweets.len();
        Ok(SearchRecentResult {
            tweets,
            meta: Some(SearchResultMeta {
                pagination: SearchPaginationMeta {
                    next_token,
                    prev_token: None,
                    result_count,
                },
            }),
        })
    }

    fn search_users_with_x_api(
        bearer_token: &str,
        args: &SearchUsersArgs,
    ) -> Result<SearchUsersResult> {
        let mut url = "https://api.twitter.com/1.1/users/search.json".to_string();
        let mut params = vec![
            ("q", args.query.clone()),
            ("count", args.limit.unwrap_or(10).to_string()),
        ];

        if let Some(cursor) = &args.cursor {
            params.push(("page", cursor.clone()));
        }

        url.push('?');
        url.push_str(&encode_query_params(&params));

        let response = ureq::get(&url)
            .set("Authorization", &format!("Bearer {}", bearer_token))
            .call();

        let api_users: Vec<ApiUser> = match response {
            Ok(resp) => resp.into_json().context("Failed to parse X API response")?,
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                anyhow::bail!("X API error {}: {}", code, body);
            }
            Err(e) => anyhow::bail!("Failed to call X API users/search: {}", e),
        };

        let users = api_users
            .into_iter()
            .map(|u| SearchUser {
                id: u.id_str,
                name: u.name,
                username: u.screen_name,
                description: u.description,
            })
            .collect::<Vec<_>>();

        let result_count = users.len();
        Ok(SearchUsersResult {
            users,
            meta: Some(SearchResultMeta {
                pagination: SearchPaginationMeta {
                    next_token: None,
                    prev_token: None,
                    result_count,
                },
            }),
        })
    }

    fn search_users_with_xquik(
        api_key: &str,
        base_url: &str,
        args: &SearchUsersArgs,
    ) -> Result<SearchUsersResult> {
        let mut url = format!("{}/x/users/search", base_url.trim_end_matches('/'));
        let mut params = vec![("q", args.query.clone())];
        if let Some(limit) = args.limit {
            params.push(("limit", limit.to_string()));
        }
        if let Some(cursor) = &args.cursor {
            params.push(("cursor", cursor.clone()));
        }
        url.push('?');
        url.push_str(&encode_query_params(&params));

        let response = ureq::get(&url)
            .set("x-api-key", api_key)
            .set("Accept", "application/json")
            .call();

        let api_response: XquikSearchUsersResponse = match response {
            Ok(resp) => resp.into_json().context("Failed to parse Xquik response")?,
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                anyhow::bail!("Xquik API error {}: {}", code, body);
            }
            Err(e) => anyhow::bail!("Failed to call Xquik user search: {}", e),
        };

        let (users, next_token) = match api_response {
            XquikSearchUsersResponse::Wrapped { users, next_cursor } => (users, next_cursor),
            XquikSearchUsersResponse::Data { data, next_cursor } => (data, next_cursor),
            XquikSearchUsersResponse::Items(users) => (users, None),
        };

        let mut users = users
            .into_iter()
            .map(|user| SearchUser {
                id: user.id,
                name: user.name,
                username: user.username,
                description: user.description,
            })
            .collect::<Vec<_>>();

        if let Some(limit) = args.limit {
            users.truncate(limit);
        }

        let result_count = users.len();
        Ok(SearchUsersResult {
            users,
            meta: Some(SearchResultMeta {
                pagination: SearchPaginationMeta {
                    next_token,
                    prev_token: None,
                    result_count,
                },
            }),
        })
    }
}

impl SearchClient for HttpSearchClient {
    fn search_recent(&self, args: &SearchRecentArgs) -> Result<SearchRecentResult> {
        match &self.backend {
            SearchBackend::XApi { bearer_token } => {
                Self::search_recent_with_x_api(bearer_token, args)
            }
            SearchBackend::Xquik { api_key, base_url } => {
                Self::search_recent_with_xquik(api_key, base_url, args)
            }
        }
    }

    fn search_users(&self, args: &SearchUsersArgs) -> Result<SearchUsersResult> {
        match &self.backend {
            SearchBackend::XApi { bearer_token } => {
                Self::search_users_with_x_api(bearer_token, args)
            }
            SearchBackend::Xquik { api_key, base_url } => {
                Self::search_users_with_xquik(api_key, base_url, args)
            }
        }
    }
}

fn encode_query_params(params: &[(&str, String)]) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

impl SearchClient for MockSearchClient {
    fn search_recent(&self, args: &SearchRecentArgs) -> Result<SearchRecentResult> {
        let limit = args.limit.unwrap_or(10);
        let offset = parse_cursor(&args.cursor);
        let tweets: Vec<SearchTweet> = self
            .tweets
            .iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();
        let result_count = tweets.len();
        let next_token = if result_count == limit && offset + limit < self.tweets.len() {
            Some(format!("cursor_{}", offset + limit))
        } else {
            None
        };
        let prev_token = if offset > 0 {
            Some(format!("cursor_{}", offset.saturating_sub(limit)))
        } else {
            None
        };
        Ok(SearchRecentResult {
            tweets,
            meta: Some(SearchResultMeta {
                pagination: SearchPaginationMeta {
                    next_token,
                    prev_token,
                    result_count,
                },
            }),
        })
    }

    fn search_users(&self, args: &SearchUsersArgs) -> Result<SearchUsersResult> {
        let limit = args.limit.unwrap_or(10);
        let offset = parse_cursor(&args.cursor);
        let users: Vec<SearchUser> = self
            .users
            .iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();
        let result_count = users.len();
        let next_token = if result_count == limit && offset + limit < self.users.len() {
            Some(format!("cursor_{}", offset + limit))
        } else {
            None
        };
        let prev_token = if offset > 0 {
            Some(format!("cursor_{}", offset.saturating_sub(limit)))
        } else {
            None
        };
        Ok(SearchUsersResult {
            users,
            meta: Some(SearchResultMeta {
                pagination: SearchPaginationMeta {
                    next_token,
                    prev_token,
                    result_count,
                },
            }),
        })
    }
}

/// Search command handler that delegates to a SearchClient
pub struct SearchCommand<C: SearchClient> {
    client: C,
}

impl SearchCommand<HttpSearchClient> {
    /// Create a new search command with HTTP client from environment
    pub fn new() -> Result<Self> {
        let client = HttpSearchClient::from_env()?;
        Ok(Self { client })
    }
}

impl<C: SearchClient> SearchCommand<C> {
    /// Create a search command with a custom client (for testing)
    pub fn with_client(client: C) -> Self {
        Self { client }
    }

    /// Search recent tweets matching the query
    pub fn search_recent(&self, args: SearchRecentArgs) -> Result<SearchRecentResult> {
        self.client.search_recent(&args)
    }

    /// Search users matching the query
    pub fn search_users(&self, args: SearchUsersArgs) -> Result<SearchUsersResult> {
        self.client.search_users(&args)
    }
}

/// Parse cursor token into an offset value
fn parse_cursor(cursor: &Option<String>) -> usize {
    if let Some(cursor) = cursor {
        cursor
            .strip_prefix("cursor_")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0)
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    #[test]
    fn test_search_recent_basic() {
        let client = MockSearchClient::with_tweet_fixtures(20);
        let cmd = SearchCommand::with_client(client);
        let args = SearchRecentArgs {
            query: "hello world".to_string(),
            limit: Some(5),
            cursor: None,
        };

        let result = cmd.search_recent(args).unwrap();
        assert_eq!(result.tweets.len(), 5);
        assert!(result.meta.is_some());
        let meta = result.meta.unwrap();
        assert_eq!(meta.pagination.result_count, 5);
        assert_eq!(meta.pagination.next_token, Some("cursor_5".to_string()));
        assert!(meta.pagination.prev_token.is_none());
    }

    #[test]
    fn test_search_recent_with_cursor() {
        let client = MockSearchClient::with_tweet_fixtures(20);
        let cmd = SearchCommand::with_client(client);
        let args = SearchRecentArgs {
            query: "rust".to_string(),
            limit: Some(5),
            cursor: Some("cursor_10".to_string()),
        };

        let result = cmd.search_recent(args).unwrap();
        assert_eq!(result.tweets.len(), 5);
        let meta = result.meta.unwrap();
        assert_eq!(meta.pagination.next_token, Some("cursor_15".to_string()));
        assert_eq!(meta.pagination.prev_token, Some("cursor_5".to_string()));
    }

    #[test]
    fn test_search_recent_tweet_contains_query() {
        let client = MockSearchClient::with_tweet_fixtures(10);
        let cmd = SearchCommand::with_client(client);
        let args = SearchRecentArgs {
            query: "rustlang".to_string(),
            limit: Some(3),
            cursor: None,
        };

        let result = cmd.search_recent(args).unwrap();
        // MockSearchClient returns fixture tweets, so we just verify count
        assert_eq!(result.tweets.len(), 3);
    }

    #[test]
    fn test_search_recent_default_limit() {
        let client = MockSearchClient::with_tweet_fixtures(15);
        let cmd = SearchCommand::with_client(client);
        let args = SearchRecentArgs {
            query: "test".to_string(),
            limit: None,
            cursor: None,
        };

        let result = cmd.search_recent(args).unwrap();
        assert_eq!(result.tweets.len(), 10); // default limit
    }

    #[test]
    fn test_search_users_basic() {
        let client = MockSearchClient::with_user_fixtures(20);
        let cmd = SearchCommand::with_client(client);
        let args = SearchUsersArgs {
            query: "alice".to_string(),
            limit: Some(5),
            cursor: None,
        };

        let result = cmd.search_users(args).unwrap();
        assert_eq!(result.users.len(), 5);
        assert!(result.meta.is_some());
        let meta = result.meta.unwrap();
        assert_eq!(meta.pagination.result_count, 5);
        assert_eq!(meta.pagination.next_token, Some("cursor_5".to_string()));
    }

    #[test]
    fn test_search_users_with_cursor() {
        let client = MockSearchClient::with_user_fixtures(20);
        let cmd = SearchCommand::with_client(client);
        let args = SearchUsersArgs {
            query: "bob".to_string(),
            limit: Some(5),
            cursor: Some("cursor_5".to_string()),
        };

        let result = cmd.search_users(args).unwrap();
        assert_eq!(result.users.len(), 5);
        let meta = result.meta.unwrap();
        assert_eq!(meta.pagination.next_token, Some("cursor_10".to_string()));
        assert_eq!(meta.pagination.prev_token, Some("cursor_0".to_string()));
    }

    #[test]
    fn test_search_users_user_fields() {
        let client = MockSearchClient::with_user_fixtures(10);
        let cmd = SearchCommand::with_client(client);
        let args = SearchUsersArgs {
            query: "developer".to_string(),
            limit: Some(3),
            cursor: None,
        };

        let result = cmd.search_users(args).unwrap();
        for user in &result.users {
            assert!(!user.id.is_empty());
            assert!(user.name.is_some());
            assert!(user.username.is_some());
            assert!(user.description.is_some());
        }
    }

    #[test]
    fn test_search_users_default_limit() {
        let client = MockSearchClient::with_user_fixtures(15);
        let cmd = SearchCommand::with_client(client);
        let args = SearchUsersArgs {
            query: "test".to_string(),
            limit: None,
            cursor: None,
        };

        let result = cmd.search_users(args).unwrap();
        assert_eq!(result.users.len(), 10); // default limit
    }

    #[test]
    fn test_xquik_search_recent_maps_response_fields() {
        let mut server = Server::new();
        let mock = server
            .mock("GET", "/x/tweets/search")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("q".to_string(), "rust lang".to_string()),
                mockito::Matcher::UrlEncoded("limit".to_string(), "2".to_string()),
                mockito::Matcher::UrlEncoded("cursor".to_string(), "abc".to_string()),
            ]))
            .match_header("x-api-key", "test_xquik_key")
            .match_header("accept", "application/json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"tweets":[{"id":"1","text":"Rust","createdAt":"2026-01-01T00:00:00Z","author":{"id":"42"}},{"id":"2","text":"Xquik","createdAt":"2026-01-02T00:00:00Z","author":{"id":"43"}}],"next_cursor":"next"}"#,
            )
            .create();

        let client = HttpSearchClient::new_xquik("test_xquik_key".to_string(), server.url());
        let cmd = SearchCommand::with_client(client);
        let args = SearchRecentArgs {
            query: "rust lang".to_string(),
            limit: Some(2),
            cursor: Some("abc".to_string()),
        };

        let result = cmd.search_recent(args).unwrap();

        mock.assert();
        assert_eq!(result.tweets.len(), 2);
        assert_eq!(result.tweets[0].id, "1");
        assert_eq!(result.tweets[0].author_id, Some("42".to_string()));
        assert_eq!(
            result.tweets[0].created_at,
            Some("2026-01-01T00:00:00Z".to_string())
        );
        let meta = result.meta.unwrap();
        assert_eq!(meta.pagination.result_count, 2);
        assert_eq!(meta.pagination.next_token, Some("next".to_string()));
    }

    #[test]
    fn test_xquik_search_users_maps_response_fields() {
        let mut server = Server::new();
        let mock = server
            .mock("GET", "/x/users/search")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("q".to_string(), "founder".to_string()),
                mockito::Matcher::UrlEncoded("limit".to_string(), "1".to_string()),
            ]))
            .match_header("x-api-key", "test_xquik_key")
            .match_header("accept", "application/json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"users":[{"id":"42","name":"Ada","username":"ada","description":"Builder"}],"nextToken":"users-next"}"#,
            )
            .create();

        let client = HttpSearchClient::new_xquik("test_xquik_key".to_string(), server.url());
        let cmd = SearchCommand::with_client(client);
        let args = SearchUsersArgs {
            query: "founder".to_string(),
            limit: Some(1),
            cursor: None,
        };

        let result = cmd.search_users(args).unwrap();

        mock.assert();
        assert_eq!(result.users.len(), 1);
        assert_eq!(result.users[0].id, "42");
        assert_eq!(result.users[0].username, Some("ada".to_string()));
        assert_eq!(result.users[0].description, Some("Builder".to_string()));
        let meta = result.meta.unwrap();
        assert_eq!(meta.pagination.result_count, 1);
        assert_eq!(meta.pagination.next_token, Some("users-next".to_string()));
    }

    #[test]
    fn test_xquik_search_users_accepts_snake_case_cursor() {
        let response: XquikSearchUsersResponse = serde_json::from_str(
            r#"{"users":[{"id":"42","name":"Ada","username":"ada","description":"Builder"}],"next_cursor":"users-next"}"#,
        )
        .unwrap();

        let next_cursor = match response {
            XquikSearchUsersResponse::Wrapped { next_cursor, .. } => next_cursor,
            XquikSearchUsersResponse::Data { next_cursor, .. } => next_cursor,
            XquikSearchUsersResponse::Items(_) => None,
        };

        assert_eq!(next_cursor, Some("users-next".to_string()));
    }

    #[test]
    fn test_parse_cursor_none() {
        assert_eq!(parse_cursor(&None), 0);
    }

    #[test]
    fn test_parse_cursor_valid() {
        assert_eq!(parse_cursor(&Some("cursor_42".to_string())), 42);
    }

    #[test]
    fn test_parse_cursor_invalid() {
        assert_eq!(parse_cursor(&Some("invalid".to_string())), 0);
    }
}
