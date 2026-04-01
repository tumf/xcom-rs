//! X API client interface and mock implementation for tweet operations.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::commands::types::{ClassifiedError, ListArgs, ListResult};
use super::models::{ConversationEdge, ConversationResult, Tweet};
use crate::x_api::XApiClient;

/// Trait representing the X API client interface for tweet operations.
/// This allows mocking in tests without real API calls.
pub trait TweetApiClient: Send + Sync {
    /// Create a tweet, optionally as a reply to another tweet.
    /// Returns the created tweet.
    fn post_tweet(&self, text: &str, reply_to: Option<&str>) -> Result<Tweet>;

    /// Fetch a single tweet by ID.
    fn get_tweet(&self, tweet_id: &str) -> Result<Tweet>;

    /// Search recent tweets matching a query.
    /// Returns a list of matching tweets.
    fn search_recent(&self, query: &str, limit: usize) -> Result<Vec<Tweet>>;

    /// List tweets for the authenticated user with field projection and pagination
    fn list_tweets(&self, args: &ListArgs) -> Result<ListResult>;
}

/// HTTP implementation of TweetApiClient using XApiClient for real API calls.
pub struct HttpTweetApiClient<T: XApiClient> {
    client: T,
}

impl<T: XApiClient> HttpTweetApiClient<T> {
    /// Create a new HTTP tweet API client with the given XApiClient
    pub fn new(client: T) -> Self {
        Self { client }
    }
}

/// Request body for creating a tweet
#[derive(Debug, Serialize)]
struct CreateTweetRequest {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply: Option<ReplySettings>,
}

#[derive(Debug, Serialize)]
struct ReplySettings {
    in_reply_to_tweet_id: String,
}

/// Response from creating a tweet
#[derive(Debug, Deserialize)]
struct CreateTweetResponse {
    data: Tweet,
}

/// Response from getting a single tweet
#[derive(Debug, Deserialize)]
struct GetTweetResponse {
    data: Tweet,
}

/// Response from searching tweets
#[derive(Debug, Deserialize)]
struct SearchTweetsResponse {
    data: Vec<Tweet>,
}

/// Response from listing tweets (with pagination)
#[derive(Debug, Deserialize)]
struct ListTweetsResponse {
    data: Option<Vec<Tweet>>,
    meta: Option<ListTweetsResponseMeta>,
}

#[derive(Debug, Deserialize)]
struct ListTweetsResponseMeta {
    #[allow(dead_code)]
    result_count: Option<usize>,
    next_token: Option<String>,
}

/// Response from /2/users/me
#[derive(Debug, Deserialize)]
struct UsersMeResponse {
    data: UsersMeData,
}

#[derive(Debug, Deserialize)]
struct UsersMeData {
    id: String,
}

impl<T: XApiClient + Send + Sync> TweetApiClient for HttpTweetApiClient<T> {
    fn post_tweet(&self, text: &str, reply_to: Option<&str>) -> Result<Tweet> {
        let request = CreateTweetRequest {
            text: text.to_string(),
            reply: reply_to.map(|id| ReplySettings {
                in_reply_to_tweet_id: id.to_string(),
            }),
        };

        let response: Result<CreateTweetResponse, _> = self.client.post("/2/tweets", &request);
        match response {
            Ok(resp) => Ok(resp.data),
            Err(error_details) => Err(ClassifiedError::from_error_details(&error_details).into()),
        }
    }

    fn get_tweet(&self, tweet_id: &str) -> Result<Tweet> {
        let path = format!(
            "/2/tweets/{}?tweet.fields=id,text,author_id,created_at,conversation_id,in_reply_to_user_id,referenced_tweets",
            tweet_id
        );
        let response: Result<GetTweetResponse, _> = self.client.get(&path);
        match response {
            Ok(resp) => Ok(resp.data),
            Err(error_details) => Err(ClassifiedError::from_error_details(&error_details).into()),
        }
    }

    fn search_recent(&self, query: &str, limit: usize) -> Result<Vec<Tweet>> {
        let path = format!(
            "/2/tweets/search/recent?query={}&max_results={}&tweet.fields=id,text,author_id,created_at,conversation_id,in_reply_to_user_id,referenced_tweets",
            urlencoding::encode(query),
            limit
        );
        let response: Result<SearchTweetsResponse, _> = self.client.get(&path);
        match response {
            Ok(resp) => Ok(resp.data),
            Err(error_details) => Err(ClassifiedError::from_error_details(&error_details).into()),
        }
    }

    fn list_tweets(&self, args: &ListArgs) -> Result<ListResult> {
        use super::commands::types::{ListResultMeta, PaginationMeta};

        // Resolve authenticated user ID
        let user_id = if let Ok(test_id) = std::env::var("XCOM_TEST_USER_ID") {
            test_id
        } else {
            let me_response: Result<UsersMeResponse, _> = self.client.get("/2/users/me");
            match me_response {
                Ok(resp) => resp.data.id,
                Err(error_details) => {
                    return Err(ClassifiedError::from_error_details(&error_details).into());
                }
            }
        };

        // Build field list from requested fields
        let field_strings: Vec<String> =
            args.fields.iter().map(|f| f.as_str().to_string()).collect();
        let fields_param = field_strings.join(",");

        let mut path = format!(
            "/2/users/{}/tweets?max_results={}&tweet.fields={}",
            user_id,
            args.limit.unwrap_or(10),
            urlencoding::encode(&fields_param)
        );

        if let Some(cursor) = &args.cursor {
            path.push_str(&format!(
                "&pagination_token={}",
                urlencoding::encode(cursor)
            ));
        }

        let response: Result<ListTweetsResponse, _> = self.client.get(&path);
        match response {
            Ok(resp) => {
                let tweets: Vec<Tweet> = resp.data.unwrap_or_default();

                // Apply field projection
                let projected_tweets = tweets
                    .into_iter()
                    .map(|t| t.project(&args.fields))
                    .collect();

                let meta = resp.meta.map(|api_meta| ListResultMeta {
                    pagination: PaginationMeta {
                        next_cursor: api_meta.next_token,
                        prev_cursor: None,
                    },
                });

                Ok(ListResult {
                    tweets: projected_tweets,
                    meta,
                })
            }
            Err(error_details) => Err(ClassifiedError::from_error_details(&error_details).into()),
        }
    }
}

/// Build conversation edges from a list of tweets.
/// Edges connect parent tweets to their replies using referenced_tweets.
pub fn build_conversation_edges(tweets: &[Tweet]) -> Vec<ConversationEdge> {
    let mut edges = Vec::new();
    for tweet in tweets {
        if let Some(refs) = &tweet.referenced_tweets {
            for r in refs {
                if r.ref_type == "replied_to" {
                    edges.push(ConversationEdge {
                        parent_id: r.id.clone(),
                        child_id: tweet.id.clone(),
                    });
                }
            }
        }
    }
    edges
}

/// Fetch a conversation tree from the API client.
/// First fetches the root tweet to get its conversation_id, then searches for
/// all tweets in the conversation and builds the tree.
/// Missing tweets referenced in edges are backfilled via individual get_tweet calls
/// to ensure thread continuity (search/recent may not return all tweets, e.g.
/// the author's own thread replies).
pub fn fetch_conversation(
    client: &dyn TweetApiClient,
    tweet_id: &str,
) -> Result<ConversationResult> {
    use std::collections::HashSet;

    // Step 1: Get root tweet to obtain conversation_id
    let root_tweet = client.get_tweet(tweet_id)?;
    let conversation_id = root_tweet
        .conversation_id
        .clone()
        .unwrap_or_else(|| tweet_id.to_string());

    // Step 2: Search for all tweets in the conversation
    let query = format!("conversation_id:{}", conversation_id);
    let mut posts = client.search_recent(&query, 100)?;

    // Ensure the root tweet is included
    if !posts.iter().any(|t| t.id == root_tweet.id) {
        posts.insert(0, root_tweet);
    }

    // Step 3: Backfill missing tweets referenced in edges.
    // search/recent may omit tweets (e.g. thread replies by the original author),
    // so we collect all referenced parent tweet IDs and fetch any that are missing.
    let mut known_ids: HashSet<String> = posts.iter().map(|t| t.id.clone()).collect();
    let mut missing_ids: Vec<String> = Vec::new();

    for post in &posts {
        if let Some(refs) = &post.referenced_tweets {
            for r in refs {
                if r.ref_type == "replied_to" && !known_ids.contains(&r.id) {
                    missing_ids.push(r.id.clone());
                    known_ids.insert(r.id.clone());
                }
            }
        }
    }

    // Fetch missing tweets individually (best-effort: skip failures)
    for missing_id in &missing_ids {
        if let Ok(tweet) = client.get_tweet(missing_id) {
            posts.push(tweet);
        }
    }

    // Sort by created_at for stable ordering
    posts.sort_by(|a, b| {
        let a_time = a.created_at.as_deref().unwrap_or("");
        let b_time = b.created_at.as_deref().unwrap_or("");
        a_time.cmp(b_time)
    });

    // Step 4: Build edges
    let edges = build_conversation_edges(&posts);

    Ok(ConversationResult {
        conversation_id,
        posts,
        edges,
    })
}
