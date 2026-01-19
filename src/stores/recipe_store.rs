//! Recipe Store
//! Handles Kind 30023 recipes with `nostrcooking` tag - caching, filtering, and publishing
//!
//! Recipe store for Kind 30023 recipe events

#![allow(dead_code)]

use dioxus::prelude::*;
use lru::LruCache;
use nostr_sdk::prelude::*;
use nostr::Event as NostrEvent;
use std::num::NonZeroUsize;
use std::time::Duration;

// Use std::result::Result to avoid ambiguity with dioxus/nostr_sdk Result types
type StdResult<T, E> = std::result::Result<T, E>;

use crate::utils::recipe::{
    RECIPE_TAG_PREFIX, RECIPE_TAG_PREFIXES, ParsedRecipe, RecipeMetadata,
    parse_recipe, extract_metadata, is_recipe_event
};

// ============================================================================
// Constants
// ============================================================================

/// Recipe events use Kind 30023 (long-form content)
pub const KIND_RECIPE: u16 = 30023;

/// Cache sizes
const RECIPE_CACHE_SIZE: usize = 200;
const CHEF_CACHE_SIZE: usize = 50;

// ============================================================================
// Data Structures
// ============================================================================

/// A cached recipe with both parsed content and metadata
#[derive(Clone, Debug, PartialEq)]
pub struct CachedRecipe {
    pub event: NostrEvent,
    pub metadata: RecipeMetadata,
    pub parsed: Option<ParsedRecipe>,
    pub naddr: String,
    pub a_tag: String,
}

/// Recipe engagement stats
#[derive(Clone, Debug, Default)]
pub struct RecipeEngagement {
    pub likes: usize,
    pub comments: usize,
    pub zaps_total_msats: u64,
}

/// Popular chef data
#[derive(Clone, Debug)]
pub struct PopularChef {
    pub pubkey: String,
    pub recipe_count: usize,
}

// ============================================================================
// Global Signals
// ============================================================================

/// Recipe cache (keyed by a_tag: "30023:pubkey:d_tag")
pub static RECIPES_CACHE: GlobalSignal<LruCache<String, CachedRecipe>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(RECIPE_CACHE_SIZE).unwrap()));

/// Popular chefs cache
pub static POPULAR_CHEFS: GlobalSignal<Vec<PopularChef>> =
    GlobalSignal::new(Vec::new);

/// Trending recipes cache (recent recipes with high engagement)
pub static TRENDING_RECIPES: GlobalSignal<Vec<CachedRecipe>> =
    GlobalSignal::new(Vec::new);

/// Loading states
pub static LOADING_RECIPES: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static LOADING_CHEFS: GlobalSignal<bool> = GlobalSignal::new(|| false);

/// Store initialization flag
pub static RECIPE_STORE_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);

// ============================================================================
// Cache Functions
// ============================================================================

/// Get a recipe from cache by a_tag
pub fn get_cached_recipe(a_tag: &str) -> Option<CachedRecipe> {
    RECIPES_CACHE.read().peek(a_tag).cloned()
}

/// Get a recipe from cache by naddr
pub fn get_cached_recipe_by_naddr(naddr: &str) -> Option<CachedRecipe> {
    let cache = RECIPES_CACHE.read();
    cache
        .iter()
        .find(|(_, r)| r.naddr == naddr)
        .map(|(_, r)| r.clone())
}

/// Cache a recipe
pub fn cache_recipe(recipe: CachedRecipe) {
    RECIPES_CACHE.write().put(recipe.a_tag.clone(), recipe);
}

/// Cache multiple recipes
pub fn cache_recipes(recipes: &[CachedRecipe]) {
    let mut cache = RECIPES_CACHE.write();
    for recipe in recipes {
        cache.put(recipe.a_tag.clone(), recipe.clone());
    }
}

/// Get all cached recipes
pub fn get_all_cached_recipes() -> Vec<CachedRecipe> {
    let cache = RECIPES_CACHE.read();
    cache.iter().map(|(_, r)| r.clone()).collect()
}

/// Clear recipe cache
pub fn clear_cache() {
    RECIPES_CACHE.write().clear();
    *TRENDING_RECIPES.write() = Vec::new();
    *POPULAR_CHEFS.write() = Vec::new();
    *RECIPE_STORE_INITIALIZED.write() = false;
}

// ============================================================================
// Parsing Functions
// ============================================================================

/// Parse a Kind 30023 event into a CachedRecipe
pub fn parse_recipe_event(event: &NostrEvent) -> Option<CachedRecipe> {
    // Must be Kind 30023
    if event.kind.as_u16() != KIND_RECIPE {
        return None;
    }

    // Must have nostrcooking tag
    if !is_recipe_event(event) {
        return None;
    }

    // Extract metadata from tags
    let metadata = extract_metadata(event);

    // Get d-tag (required)
    let d_tag = event.tags.identifier()?;

    // Build a_tag (no encoding needed - use splitn(3, ':') when parsing to handle colons in identifier)
    let a_tag = format!("{}:{}:{}", KIND_RECIPE, event.pubkey.to_hex(), d_tag);

    // Build naddr
    let naddr = match Coordinate::new(Kind::LongFormTextNote, event.pubkey)
        .identifier(d_tag)
        .to_bech32()
    {
        Ok(n) => n,
        Err(_) => return None,
    };

    // Parse content (may fail for invalid recipes)
    let parsed = parse_recipe(&event.content).ok();

    Some(CachedRecipe {
        event: event.clone(),
        metadata,
        parsed,
        naddr,
        a_tag,
    })
}

// ============================================================================
// Filter Builders
// ============================================================================

/// Build filters for all recipes (one per supported prefix)
pub fn recipes_filters(limit: usize) -> Vec<Filter> {
    RECIPE_TAG_PREFIXES
        .iter()
        .map(|prefix| {
            Filter::new()
                .kind(Kind::LongFormTextNote)
                .hashtag(*prefix)
                .limit(limit)
        })
        .collect()
}

/// Build filters for recipes by category tag (one per supported prefix)
/// e.g., category = "italian" -> #t = "nostrcooking-italian" and #t = "zapcooking-italian"
pub fn recipes_by_tag_filters(tag: &str, limit: usize, until: Option<u64>) -> Vec<Filter> {
    let tag_slug = tag.to_lowercase().replace(' ', "-");

    RECIPE_TAG_PREFIXES
        .iter()
        .map(|prefix| {
            let hashtag = format!("{}-{}", prefix, tag_slug);
            let mut filter = Filter::new()
                .kind(Kind::LongFormTextNote)
                .hashtag(&hashtag)
                .limit(limit);

            if let Some(ts) = until {
                filter = filter.until(Timestamp::from(ts));
            }

            filter
        })
        .collect()
}

/// Build filters for recipes by author (one per supported prefix)
pub fn recipes_by_author_filters(pubkey: PublicKey, limit: usize, until: Option<u64>) -> Vec<Filter> {
    RECIPE_TAG_PREFIXES
        .iter()
        .map(|prefix| {
            let mut filter = Filter::new()
                .kind(Kind::LongFormTextNote)
                .author(pubkey)
                .hashtag(*prefix)
                .limit(limit);

            if let Some(ts) = until {
                filter = filter.until(Timestamp::from(ts));
            }

            filter
        })
        .collect()
}

/// Build filter for a specific recipe by coordinate
pub fn recipe_by_coord_filter(pubkey: PublicKey, identifier: &str) -> Filter {
    Filter::new()
        .kind(Kind::LongFormTextNote)
        .author(pubkey)
        .identifier(identifier)
}

/// Build filter for recipe reactions (kind 7)
pub fn recipe_reactions_filter(a_tag: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Reaction)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), a_tag)
        .limit(limit)
}

/// Build filter for recipe comments (kind 1)
pub fn recipe_comments_filter(a_tag: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::TextNote)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), a_tag)
        .limit(limit)
}

/// Build filter for recipe zaps (kind 9735)
pub fn recipe_zaps_filter(a_tag: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::ZapReceipt)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), a_tag)
        .limit(limit)
}

/// Build filters for recent recipes (for discover section, one per supported prefix)
pub fn recent_recipes_filters(limit: usize, until: Option<u64>) -> Vec<Filter> {
    RECIPE_TAG_PREFIXES
        .iter()
        .map(|prefix| {
            let mut filter = Filter::new()
                .kind(Kind::LongFormTextNote)
                .hashtag(*prefix)
                .limit(limit);

            if let Some(ts) = until {
                filter = filter.until(Timestamp::from(ts));
            }

            filter
        })
        .collect()
}

// ============================================================================
// Fetch Functions
// ============================================================================

/// Helper to fetch events from multiple filters and deduplicate by event id
async fn fetch_with_multiple_filters(
    filters: Vec<Filter>,
    timeout: Duration,
) -> StdResult<Vec<NostrEvent>, String> {
    use std::collections::HashSet;

    let mut all_events = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut errors: Vec<String> = Vec::new();

    // Fetch from all filters concurrently
    let results = futures::future::join_all(
        filters.into_iter().map(|filter| {
            crate::stores::nostr_client::fetch_events_aggregated(filter, timeout)
        })
    ).await;

    // Merge and deduplicate, tracking errors
    for result in results {
        match result {
            Ok(events) => {
                for event in events {
                    if seen_ids.insert(event.id) {
                        all_events.push(event);
                    }
                }
            }
            Err(e) => {
                log::warn!("Recipe fetch error: {}", e);
                errors.push(e);
            }
        }
    }

    // If all requests failed, return an aggregated error
    if all_events.is_empty() && !errors.is_empty() {
        return Err(format!("All recipe fetches failed: {}", errors.join("; ")));
    }

    // Log partial failures if some succeeded but others failed
    if !errors.is_empty() {
        log::warn!("Partial recipe fetch failures ({}): {}", errors.len(), errors.join("; "));
    }

    // Sort by created_at descending (newest first)
    all_events.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(all_events)
}

/// Fetch all recipes with pagination
pub async fn fetch_recipes(limit: usize, until: Option<u64>) -> StdResult<Vec<CachedRecipe>, String> {
    *LOADING_RECIPES.write() = true;

    let filters = recent_recipes_filters(limit, until);
    let result = fetch_with_multiple_filters(filters, Duration::from_secs(15)).await;

    *LOADING_RECIPES.write() = false;

    match result {
        Ok(events) => {
            let mut recipes: Vec<CachedRecipe> = events
                .iter()
                .filter_map(parse_recipe_event)
                .collect();

            // Sort by created_at and limit
            recipes.sort_by(|a, b| b.event.created_at.cmp(&a.event.created_at));
            recipes.truncate(limit);

            cache_recipes(&recipes);
            *RECIPE_STORE_INITIALIZED.write() = true;

            log::info!("Fetched {} recipes", recipes.len());
            Ok(recipes)
        }
        Err(e) => {
            log::error!("Failed to fetch recipes: {}", e);
            Err(e)
        }
    }
}

/// Fetch recipes by tag/category with pagination
pub async fn fetch_recipes_by_tag(tag: &str, limit: usize, until: Option<u64>) -> StdResult<Vec<CachedRecipe>, String> {
    *LOADING_RECIPES.write() = true;

    let filters = recipes_by_tag_filters(tag, limit, until);
    let result = fetch_with_multiple_filters(filters, Duration::from_secs(15)).await;

    *LOADING_RECIPES.write() = false;

    match result {
        Ok(events) => {
            let mut recipes: Vec<CachedRecipe> = events
                .iter()
                .filter_map(parse_recipe_event)
                .collect();

            // Sort by created_at and limit
            recipes.sort_by(|a, b| b.event.created_at.cmp(&a.event.created_at));
            recipes.truncate(limit);

            cache_recipes(&recipes);
            log::info!("Fetched {} recipes for tag '{}'", recipes.len(), tag);
            Ok(recipes)
        }
        Err(e) => {
            log::error!("Failed to fetch recipes by tag '{}': {}", tag, e);
            Err(e)
        }
    }
}

/// Fetch recipes by author (chef) with pagination
pub async fn fetch_recipes_by_author(pubkey_hex: &str, limit: usize, until: Option<u64>) -> StdResult<Vec<CachedRecipe>, String> {
    let pubkey = PublicKey::from_hex(pubkey_hex)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    *LOADING_RECIPES.write() = true;

    let filters = recipes_by_author_filters(pubkey, limit, until);
    let result = fetch_with_multiple_filters(filters, Duration::from_secs(15)
    ).await;

    *LOADING_RECIPES.write() = false;

    match result {
        Ok(events) => {
            let mut recipes: Vec<CachedRecipe> = events
                .iter()
                .filter_map(parse_recipe_event)
                .collect();

            // Sort by created_at and limit
            recipes.sort_by(|a, b| b.event.created_at.cmp(&a.event.created_at));
            recipes.truncate(limit);

            cache_recipes(&recipes);
            log::info!("Fetched {} recipes by author", recipes.len());
            Ok(recipes)
        }
        Err(e) => {
            log::error!("Failed to fetch recipes by author: {}", e);
            Err(e)
        }
    }
}

/// Fetch a specific recipe by naddr
pub async fn fetch_recipe_by_naddr(naddr: &str) -> StdResult<Option<CachedRecipe>, String> {
    // Check cache first
    if let Some(cached) = get_cached_recipe_by_naddr(naddr) {
        return Ok(Some(cached));
    }

    // Parse naddr
    let coord = Coordinate::from_bech32(naddr)
        .map_err(|e| format!("Invalid naddr: {}", e))?;

    let filter = recipe_by_coord_filter(coord.public_key, &coord.identifier);

    let events = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(10)
    ).await?;

    if let Some(event) = events.first() {
        if let Some(recipe) = parse_recipe_event(event) {
            cache_recipe(recipe.clone());
            return Ok(Some(recipe));
        }
    }

    Ok(None)
}

/// Fetch recipe engagement stats
pub async fn fetch_recipe_engagement(a_tag: &str) -> StdResult<RecipeEngagement, String> {
    let reactions_filter = recipe_reactions_filter(a_tag, 500);
    let comments_filter = recipe_comments_filter(a_tag, 200);
    let zaps_filter = recipe_zaps_filter(a_tag, 100);

    let (reactions_result, comments_result, zaps_result) = futures::join!(
        crate::stores::nostr_client::fetch_events_aggregated(reactions_filter, Duration::from_secs(5)),
        crate::stores::nostr_client::fetch_events_aggregated(comments_filter, Duration::from_secs(5)),
        crate::stores::nostr_client::fetch_events_aggregated(zaps_filter, Duration::from_secs(5)),
    );

    let likes = reactions_result
        .map(|events| events.iter().filter(|e| {
            // Count only positive reactions
            let content = &e.content;
            content.is_empty() || content == "+" || content == "like" ||
            content.chars().next().map(|c| !c.is_ascii() || c == '+').unwrap_or(true)
        }).count())
        .unwrap_or(0);

    let comments = comments_result
        .map(|events| events.len())
        .unwrap_or(0);

    let zaps_total_msats = zaps_result
        .map(|events| {
            events.iter().filter_map(|e| {
                // Extract amount from bolt11 tag or description
                e.tags.iter()
                    .find(|t| t.kind().to_string() == "bolt11")
                    .and_then(|t| t.content())
                    .and({
                        // Parse amount from bolt11 - simplified
                        // Real implementation would use lightning-invoice crate
                        None::<u64>
                    })
            }).sum::<u64>()
        })
        .unwrap_or(0);

    Ok(RecipeEngagement {
        likes,
        comments,
        zaps_total_msats,
    })
}

/// Fetch popular chefs (authors with most recipes)
pub async fn fetch_popular_chefs(limit: usize) -> StdResult<Vec<PopularChef>, String> {
    *LOADING_CHEFS.write() = true;

    // Fetch a large batch of recipes to count authors
    let filters = recipes_filters(500);
    let result = fetch_with_multiple_filters(filters, Duration::from_secs(20)).await;

    *LOADING_CHEFS.write() = false;

    match result {
        Ok(events) => {
            // Count recipes per author
            let mut author_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

            for event in &events {
                if is_recipe_event(event) {
                    let pubkey = event.pubkey.to_hex();
                    *author_counts.entry(pubkey).or_insert(0) += 1;
                }
            }

            // Sort by count and take top N
            let mut chefs: Vec<PopularChef> = author_counts
                .into_iter()
                .map(|(pubkey, recipe_count)| PopularChef { pubkey, recipe_count })
                .collect();

            chefs.sort_by(|a, b| b.recipe_count.cmp(&a.recipe_count));
            chefs.truncate(limit);

            // Cache
            *POPULAR_CHEFS.write() = chefs.clone();

            log::info!("Fetched {} popular chefs", chefs.len());
            Ok(chefs)
        }
        Err(e) => {
            log::error!("Failed to fetch popular chefs: {}", e);
            Err(e)
        }
    }
}

/// Fetch trending/discover recipes (recent with validation)
pub async fn fetch_discover_recipes(limit: usize) -> StdResult<Vec<CachedRecipe>, String> {
    let recipes = fetch_recipes(limit * 2, None).await?;

    // Filter to only valid recipes (with parsed content)
    let valid_recipes: Vec<CachedRecipe> = recipes
        .into_iter()
        .filter(|r| r.parsed.is_some())
        .take(limit)
        .collect();

    // Update trending cache
    *TRENDING_RECIPES.write() = valid_recipes.clone();

    Ok(valid_recipes)
}

/// Fetch trending recipes (recent valid recipes with images)
/// Prioritizes recipes with images and valid parsed content
pub async fn fetch_trending_recipes(limit: usize) -> StdResult<Vec<CachedRecipe>, String> {
    // Fetch more than needed to filter for quality
    let recipes = fetch_recipes(limit * 3, None).await?;

    // Filter and sort: prefer recipes with images, then by recency
    let mut valid_recipes: Vec<CachedRecipe> = recipes
        .into_iter()
        .filter(|r| r.parsed.is_some())
        .collect();

    // Sort: recipes with images first, then by created_at descending
    valid_recipes.sort_by(|a, b| {
        let a_has_image = !a.metadata.images.is_empty();
        let b_has_image = !b.metadata.images.is_empty();

        match (a_has_image, b_has_image) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => b.event.created_at.cmp(&a.event.created_at),
        }
    });

    valid_recipes.truncate(limit);

    // Update trending cache
    *TRENDING_RECIPES.write() = valid_recipes.clone();

    Ok(valid_recipes)
}

/// Search recipes by title, summary, or content
/// Supports tag aliases (e.g., searching "bbq" matches "Barbecue")
pub async fn search_recipes(query: &str, limit: usize) -> StdResult<Vec<CachedRecipe>, String> {
    let query_lower = query.to_lowercase();

    // Normalize query to canonical tag name if it's an alias
    // e.g., "bbq" -> "Barbecue", "spice" -> "Spicy"
    let normalized_query = crate::utils::recipe_tags::normalize_tag(query);
    let normalized_lower = normalized_query.to_lowercase();

    // Fetch a large batch to search through
    let recipes = fetch_recipes(500, None).await?;

    // Filter recipes matching the query
    let matching_recipes: Vec<CachedRecipe> = recipes
        .into_iter()
        .filter(|r| {
            // Match against title
            if r.metadata.title.to_lowercase().contains(&query_lower) {
                return true;
            }

            // Match against summary
            if let Some(ref summary) = r.metadata.summary {
                if summary.to_lowercase().contains(&query_lower) {
                    return true;
                }
            }

            // Match against tags (check both original query and normalized alias)
            for tag in &r.metadata.tags {
                let tag_lower = tag.to_lowercase();
                if tag_lower.contains(&query_lower) {
                    return true;
                }
                // Also match normalized alias (e.g., searching "bbq" matches "Barbecue")
                if normalized_lower != query_lower && tag_lower.contains(&normalized_lower) {
                    return true;
                }
            }

            // Match against content (ingredients, directions)
            if r.event.content.to_lowercase().contains(&query_lower) {
                return true;
            }

            false
        })
        .take(limit)
        .collect();

    log::info!("Search '{}' found {} recipes (normalized: '{}')", query, matching_recipes.len(), normalized_query);
    Ok(matching_recipes)
}

// ============================================================================
// Publishing Functions
// ============================================================================

/// Publish a new recipe
pub async fn publish_recipe(
    title: &str,
    summary: Option<&str>,
    image_urls: &[String],
    content: &str,
    tags: Vec<String>,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Generate slug from title (compatible format - only replaces spaces)
    let slug = crate::utils::recipe_slug(title);

    // Build tags
    let mut event_tags: Vec<Tag> = vec![
        Tag::identifier(&slug),
        Tag::title(title),
        Tag::hashtag(RECIPE_TAG_PREFIX),
        // Title-based hashtag for discoverability
        Tag::hashtag(format!("{}-{}", RECIPE_TAG_PREFIX, &slug)),
        Tag::custom(
            TagKind::Custom("published_at".into()),
            vec![Timestamp::now().as_secs().to_string()]
        ),
    ];

    if let Some(s) = summary {
        event_tags.push(Tag::custom(
            TagKind::Custom("summary".into()),
            vec![s.to_string()]
        ));
    }

    // Add multiple images
    for img in image_urls {
        if let Ok(url) = Url::parse(img) {
            event_tags.push(Tag::image(url, None));
        }
    }

    // Add category tags with nostrcooking prefix
    for tag in tags {
        let tag_slug = tag.to_lowercase().replace(' ', "-");
        event_tags.push(Tag::hashtag(format!("{}-{}", RECIPE_TAG_PREFIX, tag_slug)));
    }

    // Build and publish
    let builder = EventBuilder::long_form_text_note(content)
        .tags(event_tags);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish recipe: {}", e))?;

    log::info!("Recipe published: {}", output.id().to_hex());

    // Return naddr instead of hex id
    let pubkey = crate::stores::nostr_client::get_cached_pubkey()?;
    let naddr = crate::stores::nostr_client::make_naddr_with_hints(KIND_RECIPE, &pubkey, &slug).await?;
    Ok(naddr)
}

/// Fork (copy) an existing recipe for editing
pub async fn fork_recipe(
    original: &CachedRecipe,
    new_title: &str,
    new_content: &str,
    new_tags: Vec<String>,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Generate new slug (compatible format - only replaces spaces)
    let slug = crate::utils::recipe_slug(new_title);

    // Build tags - include reference to original
    let mut event_tags: Vec<Tag> = vec![
        Tag::identifier(&slug),
        Tag::title(new_title),
        Tag::hashtag(RECIPE_TAG_PREFIX),
        // Title-based hashtag for discoverability
        Tag::hashtag(format!("{}-{}", RECIPE_TAG_PREFIX, &slug)),
        Tag::custom(
            TagKind::Custom("published_at".into()),
            vec![Timestamp::now().as_secs().to_string()]
        ),
        // Reference to forked recipe
        Tag::custom(
            TagKind::Custom("forked-from".into()),
            vec![original.a_tag.clone()]
        ),
        // Credit original author
        Tag::public_key(original.event.pubkey),
    ];

    // Keep original images if available (supports multiple images)
    for img in &original.metadata.images {
        if let Ok(url) = Url::parse(img) {
            event_tags.push(Tag::image(url, None));
        }
    }

    // Add category tags
    for tag in new_tags {
        let tag_slug = tag.to_lowercase().replace(' ', "-");
        event_tags.push(Tag::hashtag(format!("{}-{}", RECIPE_TAG_PREFIX, tag_slug)));
    }

    // Build and publish
    let builder = EventBuilder::long_form_text_note(new_content)
        .tags(event_tags);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to fork recipe: {}", e))?;

    log::info!("Recipe forked: {}", output.id().to_hex());

    // Return naddr instead of hex id
    let pubkey = crate::stores::nostr_client::get_cached_pubkey()?;
    let naddr = crate::stores::nostr_client::make_naddr_with_hints(KIND_RECIPE, &pubkey, &slug).await?;
    Ok(naddr)
}

/// Update an existing recipe (republish with same d-tag)
pub async fn update_recipe(
    original_slug: &str,
    title: &str,
    summary: Option<&str>,
    image_urls: &[String],
    content: &str,
    tags: Vec<String>,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Build tags - use original slug to update
    let mut event_tags: Vec<Tag> = vec![
        Tag::identifier(original_slug),
        Tag::title(title),
        Tag::hashtag(RECIPE_TAG_PREFIX),
        // Title-based hashtag for discoverability
        Tag::hashtag(format!("{}-{}", RECIPE_TAG_PREFIX, original_slug)),
        Tag::custom(
            TagKind::Custom("published_at".into()),
            vec![Timestamp::now().as_secs().to_string()]
        ),
    ];

    if let Some(s) = summary {
        event_tags.push(Tag::custom(
            TagKind::Custom("summary".into()),
            vec![s.to_string()]
        ));
    }

    // Add multiple images
    for img in image_urls {
        if let Ok(url) = Url::parse(img) {
            event_tags.push(Tag::image(url, None));
        }
    }

    // Add category tags
    for tag in tags {
        let tag_slug = tag.to_lowercase().replace(' ', "-");
        event_tags.push(Tag::hashtag(format!("{}-{}", RECIPE_TAG_PREFIX, tag_slug)));
    }

    // Build and publish
    let builder = EventBuilder::long_form_text_note(content)
        .tags(event_tags);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to update recipe: {}", e))?;

    log::info!("Recipe updated: {}", output.id().to_hex());

    // Return naddr instead of hex id
    let pubkey = crate::stores::nostr_client::get_cached_pubkey()?;
    let naddr = crate::stores::nostr_client::make_naddr_with_hints(KIND_RECIPE, &pubkey, original_slug).await?;
    Ok(naddr)
}

/// Delete a recipe (publish deletion event)
pub async fn delete_recipe(recipe: &CachedRecipe) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Verify ownership
    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    if recipe.event.pubkey.to_hex() != current_pubkey {
        return Err("You can only delete your own recipes".to_string());
    }

    // Build deletion event using EventDeletionRequest
    let coord = Coordinate::new(Kind::LongFormTextNote, recipe.event.pubkey)
        .identifier(recipe.metadata.identifier.as_deref().unwrap_or(""));

    let deletion_request = EventDeletionRequest::new()
        .coordinate(coord);

    let builder = EventBuilder::delete(deletion_request);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to delete recipe: {}", e))?;

    // Remove from cache
    RECIPES_CACHE.write().pop(&recipe.a_tag);

    log::info!("Recipe deleted: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

// ============================================================================
// Comment Functions
// ============================================================================

/// Post a comment on a recipe
pub async fn comment_on_recipe(recipe: &CachedRecipe, content: &str) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let d_tag = recipe.metadata.identifier.as_deref()
        .ok_or("Recipe has no identifier")?;

    // Build coordinate
    let coord = Coordinate::new(Kind::LongFormTextNote, recipe.event.pubkey)
        .identifier(d_tag);

    // Build tags for comment
    let tags: Vec<Tag> = vec![
        Tag::coordinate(coord, None),
        Tag::public_key(recipe.event.pubkey),
    ];

    let builder = EventBuilder::text_note(content)
        .tags(tags);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to post comment: {}", e))?;

    log::info!("Comment posted: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

// ============================================================================
// Bookmark Functions (NIP-51 Lists)
// ============================================================================

/// Recipe bookmark list kind (NIP-51)
pub const KIND_BOOKMARK_LIST: u16 = 30001;

/// Fetch user's recipe bookmarks
pub async fn fetch_recipe_bookmarks(user_pubkey: &str) -> StdResult<Vec<String>, String> {
    let pubkey = PublicKey::from_hex(user_pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Filter for bookmark lists with nostrcooking tag
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_BOOKMARK_LIST))
        .author(pubkey)
        .hashtag(RECIPE_TAG_PREFIX)
        .limit(1);

    let events = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(5)
    ).await?;

    // Extract a-tags from the bookmark list
    if let Some(event) = events.first() {
        let bookmarks: Vec<String> = event.tags.iter()
            .filter(|t| t.kind() == TagKind::a())
            .filter_map(|t| t.content().map(|s| s.to_string()))
            .filter(|a| a.starts_with(&format!("{}:", KIND_RECIPE)))
            .collect();

        return Ok(bookmarks);
    }

    Ok(Vec::new())
}

/// Toggle recipe bookmark
pub async fn toggle_recipe_bookmark(recipe: &CachedRecipe) -> StdResult<bool, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    // Fetch current bookmarks
    let mut bookmarks = fetch_recipe_bookmarks(&current_pubkey).await?;

    // Toggle
    let is_bookmarked = bookmarks.contains(&recipe.a_tag);
    if is_bookmarked {
        bookmarks.retain(|b| b != &recipe.a_tag);
    } else {
        bookmarks.push(recipe.a_tag.clone());
    }

    // Build new bookmark list
    let mut tags: Vec<Tag> = vec![
        Tag::identifier("recipes"),
        Tag::hashtag(RECIPE_TAG_PREFIX),
    ];

    for bookmark in &bookmarks {
        tags.push(Tag::custom(TagKind::a(), vec![bookmark.clone()]));
    }

    let builder = EventBuilder::new(Kind::Custom(KIND_BOOKMARK_LIST), "")
        .tags(tags);

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to update bookmarks: {}", e))?;

    log::info!("Bookmark toggled for {}: {}", recipe.a_tag, !is_bookmarked);
    Ok(!is_bookmarked)
}

/// Check if recipe is bookmarked
pub async fn is_recipe_bookmarked(recipe: &CachedRecipe, user_pubkey: &str) -> StdResult<bool, String> {
    let bookmarks = fetch_recipe_bookmarks(user_pubkey).await?;
    Ok(bookmarks.contains(&recipe.a_tag))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get recipe coordinate string
pub fn get_recipe_coordinate(recipe: &CachedRecipe) -> String {
    recipe.a_tag.clone()
}

/// Get recipe naddr for sharing
pub fn get_recipe_naddr(recipe: &CachedRecipe) -> String {
    recipe.naddr.clone()
}

/// Get shareable URL for a recipe
pub fn get_recipe_url(recipe: &CachedRecipe) -> String {
    format!("/recipes/{}", recipe.naddr)
}

// ============================================================================
// Explore Page Functions
// ============================================================================

/// Tag with usage count
#[derive(Clone, Debug)]
pub struct TagWithCount {
    pub tag: String,
    pub count: usize,
}

/// Collection for the explore page
#[derive(Clone, Debug)]
pub struct Collection {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub tag: String,
    pub image_url: Option<String>,
}

/// Static collection definitions
pub const STATIC_COLLECTIONS: &[(&str, &str, &str, &str)] = &[
    ("breakfast", "Breakfast Favorites", "Recipes", "Breakfast"),
    ("dessert", "Sweet Treats", "Recipes", "Dessert"),
    ("quick", "Quick & Easy", "Recipes", "Quick"),
    ("italian", "Italian Classics", "Recipes", "Italian"),
    ("healthy", "Healthy Choices", "Recipes", "Healthy"),
];

/// Fetch a sample recipe image for a collection tag
pub async fn fetch_collection_image(tag: &str) -> StdResult<Option<String>, String> {
    let filters = recipes_by_tag_filters(tag, 5, None);
    let events = fetch_with_multiple_filters(filters, Duration::from_secs(3)).await?;

    // Find first recipe with an image
    for event in events {
        if let Some(recipe) = parse_recipe_event(&event) {
            if let Some(img) = recipe.metadata.primary_image() {
                return Ok(Some(img.clone()));
            }
        }
    }

    Ok(None)
}

/// Fetch collections with images
pub async fn fetch_collections_with_images() -> Vec<Collection> {
    let mut collections = Vec::new();

    for (id, title, subtitle, tag) in STATIC_COLLECTIONS {
        // Try to fetch an image for this collection
        let image_url = fetch_collection_image(tag).await.ok().flatten();

        collections.push(Collection {
            id: id.to_string(),
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            tag: tag.to_string(),
            image_url,
        });
    }

    collections
}

/// Compute popular tags with counts
pub async fn compute_popular_tags(limit: usize) -> StdResult<Vec<TagWithCount>, String> {
    // Fetch a large batch of recipes from all supported prefixes
    let filters = recipes_filters(500);
    let events = fetch_with_multiple_filters(filters, Duration::from_secs(15)).await?;

    // Count tag usage
    let mut tag_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let prefixes: Vec<String> = RECIPE_TAG_PREFIXES.iter().map(|p| format!("{}-", p)).collect();

    for event in &events {
        if !is_recipe_event(event) {
            continue;
        }

        // Count each hashtag that starts with nostrcooking- or zapcooking-
        for tag in event.tags.iter() {
            if tag.kind() == TagKind::t() {
                if let Some(content) = tag.content() {
                    // Try to strip any of the supported prefixes
                    let tag_name = prefixes.iter()
                        .find_map(|prefix| content.strip_prefix(prefix.as_str()));

                    if let Some(tag_name) = tag_name {
                        // Convert to title case
                        let display_name = tag_name
                            .split('-')
                            .map(|s| {
                                let mut chars = s.chars();
                                match chars.next() {
                                    Some(c) => c.to_uppercase().chain(chars).collect(),
                                    None => String::new(),
                                }
                            })
                            .collect::<Vec<String>>()
                            .join(" ");

                        *tag_counts.entry(display_name).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    // Sort by count and take top N
    let mut tags: Vec<TagWithCount> = tag_counts
        .into_iter()
        .map(|(tag, count)| TagWithCount { tag, count })
        .collect();

    tags.sort_by(|a, b| b.count.cmp(&a.count));
    tags.truncate(limit);

    log::info!("Computed {} popular tags", tags.len());
    Ok(tags)
}
