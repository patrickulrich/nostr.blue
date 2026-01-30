// Recipe parsing and validation utilities
// Kind 30023 recipes with nostrcooking tag

use nostr::Event;
use regex::Regex;

/// Base tag prefix for recipe filtering (used when publishing)
pub const RECIPE_TAG_PREFIX: &str = "nostrcooking";

/// Alternative tag prefix (zap.cooking uses this)
pub const RECIPE_TAG_PREFIX_ALT: &str = "zapcooking";

/// All supported recipe tag prefixes for reading/filtering
pub const RECIPE_TAG_PREFIXES: &[&str] = &[RECIPE_TAG_PREFIX, RECIPE_TAG_PREFIX_ALT];

/// Recipe details extracted from the Details section
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecipeDetails {
    pub prep_time: Option<String>,
    pub cook_time: Option<String>,
    pub servings: Option<String>,
}

/// Parsed recipe content from markdown
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ParsedRecipe {
    pub chef_notes: Option<String>,
    pub details: RecipeDetails,
    pub ingredients: Vec<String>,
    pub directions: Vec<String>,
    pub additional_resources: Option<String>,
}

/// Recipe metadata extracted from event tags
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecipeMetadata {
    pub title: String,
    pub summary: Option<String>,
    /// All images from the recipe (supports multiple images)
    pub images: Vec<String>,
    pub identifier: Option<String>,
    pub published_at: u64,
    pub tags: Vec<String>,
}

impl RecipeMetadata {
    /// Get primary (first) image for display in cards/thumbnails
    pub fn primary_image(&self) -> Option<&String> {
        self.images.first()
    }
}

/// Validation error types
#[derive(Clone, Debug, PartialEq)]
pub enum ValidationError {
    MissingSections,
    NoIngredients,
    NoDirections,
    InvalidDirectionFormat(String),
    ChefNotesTooLong,
    PrepTimeTooLong,
    CookTimeTooLong,
    ServingsTooLong,
    IngredientTooLong,
    DirectionTooLong,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::MissingSections => write!(f, "Recipe sections are missing"),
            ValidationError::NoIngredients => write!(f, "At least one ingredient is required"),
            ValidationError::NoDirections => write!(f, "At least one direction step is required"),
            ValidationError::InvalidDirectionFormat(msg) => {
                write!(f, "Invalid direction format: {}", msg)
            }
            ValidationError::ChefNotesTooLong => write!(f, "Chef's notes exceed character limit"),
            ValidationError::PrepTimeTooLong => write!(f, "Prep time exceeds character limit"),
            ValidationError::CookTimeTooLong => write!(f, "Cook time exceeds character limit"),
            ValidationError::ServingsTooLong => write!(f, "Servings exceed character limit"),
            ValidationError::IngredientTooLong => {
                write!(f, "An ingredient exceeds character limit")
            }
            ValidationError::DirectionTooLong => {
                write!(f, "A direction step exceeds character limit")
            }
        }
    }
}

/// Parse and validate recipe markdown content
/// Returns the parsed recipe or a validation error
pub fn parse_recipe(markdown: &str) -> Result<ParsedRecipe, ValidationError> {
    let mut recipe = ParsedRecipe::default();

    // Split content by section headers (## )
    // Process each section by finding headers and their content
    let mut found_sections = false;
    let lines: Vec<&str> = markdown.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Check if this line is a section header
        if line.starts_with("## ") {
            found_sections = true;
            let section_name = line.trim_start_matches("## ").trim();

            // Collect content until next section or end
            let mut content_lines = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].starts_with("## ") {
                content_lines.push(lines[i]);
                i += 1;
            }
            let content = content_lines.join("\n").trim().to_string();

            match section_name {
                "Chef's notes" => {
                    if content.len() > 99999 {
                        return Err(ValidationError::ChefNotesTooLong);
                    }
                    recipe.chef_notes = Some(content.to_string());
                }
                "Details" => {
                    parse_details(&content, &mut recipe.details)?;
                }
                "Ingredients" => {
                    recipe.ingredients = parse_ingredients(&content)?;
                }
                "Directions" => {
                    recipe.directions = parse_directions(&content)?;
                }
                "Additional Resources" => {
                    recipe.additional_resources = Some(content.to_string());
                }
                _ => {}
            }
        } else {
            i += 1;
        }
    }

    if !found_sections {
        return Err(ValidationError::MissingSections);
    }

    if recipe.ingredients.is_empty() {
        return Err(ValidationError::NoIngredients);
    }

    if recipe.directions.is_empty() {
        return Err(ValidationError::NoDirections);
    }

    Ok(recipe)
}

/// Parse the Details section content
fn parse_details(content: &str, details: &mut RecipeDetails) -> Result<(), ValidationError> {
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("- ") {
            if let Some((key, value)) = rest.split_once(": ") {
                let value = value.trim();
                match key {
                    "Prep time" | "Prep Time" => {
                        if value.len() > 999 {
                            return Err(ValidationError::PrepTimeTooLong);
                        }
                        details.prep_time = Some(value.to_string());
                    }
                    "Cook time" | "Cook Time" => {
                        if value.len() > 999 {
                            return Err(ValidationError::CookTimeTooLong);
                        }
                        details.cook_time = Some(value.to_string());
                    }
                    "Servings" => {
                        if value.len() > 999 {
                            return Err(ValidationError::ServingsTooLong);
                        }
                        details.servings = Some(value.to_string());
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

/// Parse ingredients from bullet list
fn parse_ingredients(content: &str) -> Result<Vec<String>, ValidationError> {
    let mut ingredients = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if let Some(stripped) = line.strip_prefix("- ") {
            let ingredient = stripped.trim();
            if ingredient.len() > 9999 {
                return Err(ValidationError::IngredientTooLong);
            }
            if !ingredient.is_empty() {
                ingredients.push(ingredient.to_string());
            }
        }
    }
    Ok(ingredients)
}

/// Parse directions from numbered list (supports continuation lines)
fn parse_directions(content: &str) -> Result<Vec<String>, ValidationError> {
    let mut directions: Vec<String> = Vec::new();
    let number_re = Regex::new(r"^(\d+)\.\s*(.*)$").unwrap();
    let mut expected_step = 1;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(cap) = number_re.captures(line) {
            let step_num: usize = cap.get(1).unwrap().as_str().parse().unwrap_or(0);
            let step_text = cap.get(2).map(|m| m.as_str().trim()).unwrap_or("");

            if step_num != expected_step {
                return Err(ValidationError::InvalidDirectionFormat(format!(
                    "Expected step {}, found step {}",
                    expected_step, step_num
                )));
            }

            // Start a new step
            directions.push(step_text.to_string());
            expected_step += 1;
        } else {
            // Continuation line - append to current step
            if let Some(current_step) = directions.last_mut() {
                if current_step.is_empty() {
                    *current_step = line.to_string();
                } else {
                    current_step.push('\n');
                    current_step.push_str(line);
                }
            }
            // If no current step exists, ignore orphan continuation lines
        }
    }

    // Validate total length of each direction
    for direction in &directions {
        if direction.len() > 9999 {
            return Err(ValidationError::DirectionTooLong);
        }
    }

    // Remove empty directions
    directions.retain(|d| !d.trim().is_empty());

    Ok(directions)
}

/// Extract recipe metadata from a Kind 30023 event
#[allow(clippy::field_reassign_with_default)]
pub fn extract_metadata(event: &Event) -> RecipeMetadata {
    let mut meta = RecipeMetadata::default();

    // Title
    meta.title = event
        .tags
        .iter()
        .find(|tag| tag.kind().to_string() == "title")
        .and_then(|tag| tag.content())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Untitled Recipe".to_string());

    // Summary
    meta.summary = event
        .tags
        .iter()
        .find(|tag| tag.kind().to_string() == "summary")
        .and_then(|tag| tag.content())
        .map(|s| s.to_string());

    // Images (collect all image tags for multiple image support)
    meta.images = event
        .tags
        .iter()
        .filter(|tag| tag.kind().to_string() == "image")
        .filter_map(|tag| tag.content().map(|s| s.to_string()))
        .collect();

    // Identifier (d tag)
    meta.identifier = event.tags.identifier().map(|s| s.to_string());

    // Published at
    meta.published_at = event
        .tags
        .iter()
        .find(|tag| tag.kind().to_string() == "published_at")
        .and_then(|tag| tag.content())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| event.created_at.as_secs());

    // Recipe-specific tags (filter nostrcooking- or zapcooking- prefix)
    meta.tags = event
        .tags
        .hashtags()
        .filter(|tag| {
            RECIPE_TAG_PREFIXES
                .iter()
                .any(|prefix| tag.starts_with(prefix) && *tag != *prefix)
        })
        .map(|tag| {
            // Remove prefix to get clean tag name (try all prefixes)
            for prefix in RECIPE_TAG_PREFIXES {
                if let Some(stripped) = tag.strip_prefix(&format!("{}-", prefix)) {
                    return stripped.to_string();
                }
            }
            tag.to_string()
        })
        .collect();

    meta
}

/// Check if an event is a recipe (has nostrcooking or zapcooking tag)
pub fn is_recipe_event(event: &Event) -> bool {
    event
        .tags
        .hashtags()
        .any(|tag| RECIPE_TAG_PREFIXES.contains(&tag))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_RECIPE: &str = r#"## Chef's notes

This is a simple test recipe.

## Details

- Prep time: 10 minutes
- Cook time: 20 minutes
- Servings: 4

## Ingredients

- 2 cups flour
- 1 cup sugar
- 1 egg

## Directions

1. Mix dry ingredients
2. Add wet ingredients
3. Bake at 350F

## Additional Resources

Check out my other recipes!
"#;

    #[test]
    fn test_parse_valid_recipe() {
        let result = parse_recipe(VALID_RECIPE);
        assert!(result.is_ok());

        let recipe = result.unwrap();
        assert_eq!(
            recipe.chef_notes,
            Some("This is a simple test recipe.".to_string())
        );
        assert_eq!(recipe.details.prep_time, Some("10 minutes".to_string()));
        assert_eq!(recipe.details.cook_time, Some("20 minutes".to_string()));
        assert_eq!(recipe.details.servings, Some("4".to_string()));
        assert_eq!(recipe.ingredients.len(), 3);
        assert_eq!(recipe.directions.len(), 3);
        assert!(recipe.additional_resources.is_some());
    }

    #[test]
    fn test_minimal_recipe() {
        let minimal = r#"## Ingredients

- Water

## Directions

1. Pour water
"#;
        let result = parse_recipe(minimal);
        assert!(result.is_ok());
        let recipe = result.unwrap();
        assert_eq!(recipe.ingredients.len(), 1);
        assert_eq!(recipe.directions.len(), 1);
    }

    #[test]
    fn test_missing_ingredients() {
        let no_ingredients = r#"## Directions

1. Do something
"#;
        let result = parse_recipe(no_ingredients);
        assert!(matches!(result, Err(ValidationError::NoIngredients)));
    }

    #[test]
    fn test_missing_directions() {
        let no_directions = r#"## Ingredients

- Something
"#;
        let result = parse_recipe(no_directions);
        assert!(matches!(result, Err(ValidationError::NoDirections)));
    }
}
