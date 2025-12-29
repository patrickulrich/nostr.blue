// Recipe tags for the /recipes feature
// Uses "nostrcooking" tag prefix for Kind 30023 recipes

/// A recipe category tag with optional emoji
#[derive(Clone, Debug, PartialEq)]
pub struct RecipeTag {
    /// Display name (e.g., "Italian")
    pub name: &'static str,
    /// Slug for nostr tag (e.g., "italian") - lowercase, hyphenated
    pub slug: &'static str,
    /// Optional emoji for display
    pub emoji: Option<&'static str>,
}

impl RecipeTag {
    pub const fn new(name: &'static str, slug: &'static str, emoji: Option<&'static str>) -> Self {
        Self { name, slug, emoji }
    }
}

/// All 163 curated recipe tags
/// These are used for filtering and categorizing recipes
pub static RECIPE_TAGS: &[RecipeTag] = &[
    RecipeTag::new("Alcohol", "alcohol", Some("🍸")),
    RecipeTag::new("Almond", "almond", None),
    RecipeTag::new("American", "american", Some("🇺🇸")),
    RecipeTag::new("Apple", "apple", Some("🍎")),
    RecipeTag::new("Argentinian", "argentinian", Some("🇦🇷")),
    RecipeTag::new("Asian", "asian", Some("🥢")),
    RecipeTag::new("Australian", "australian", Some("🇦🇺")),
    RecipeTag::new("Austrian", "austrian", Some("🇦🇹")),
    RecipeTag::new("Bacon", "bacon", Some("🥓")),
    RecipeTag::new("Baked", "baked", None),
    RecipeTag::new("Bariatric", "bariatric", None),
    RecipeTag::new("Basic", "basic", None),
    RecipeTag::new("Beans", "beans", Some("🧆")),
    RecipeTag::new("Beef", "beef", Some("🐄")),
    RecipeTag::new("Beetroot", "beetroot", None),
    RecipeTag::new("Belgian", "belgian", Some("🇧🇪")),
    RecipeTag::new("Blended", "blended", None),
    RecipeTag::new("Brazilian", "brazilian", Some("🇧🇷")),
    RecipeTag::new("Bread", "bread", Some("🍞")),
    RecipeTag::new("Breakfast", "breakfast", Some("🍳")),
    RecipeTag::new("Broccoli", "broccoli", None),
    RecipeTag::new("Built", "built", None),
    RecipeTag::new("Cabbage", "cabbage", None),
    RecipeTag::new("Cajun", "cajun", None),
    RecipeTag::new("Cake", "cake", Some("🍰")),
    RecipeTag::new("Cheese", "cheese", Some("🧀")),
    RecipeTag::new("Cheesefare", "cheesefare", None),
    RecipeTag::new("Chicken", "chicken", Some("🍗")),
    RecipeTag::new("Chinese", "chinese", Some("🥡")),
    RecipeTag::new("Chocolate", "chocolate", Some("🍫")),
    RecipeTag::new("Christmas", "christmas", Some("🎄")),
    RecipeTag::new("Cocktail", "cocktail", Some("🍹")),
    RecipeTag::new("Coconut", "coconut", Some("🥥")),
    RecipeTag::new("Cookies", "cookies", Some("🍪")),
    RecipeTag::new("Coffee", "coffee", Some("☕")),
    RecipeTag::new("Corn", "corn", Some("🌽")),
    RecipeTag::new("Cream", "cream", Some("🥛")),
    RecipeTag::new("Curry", "curry", None),
    RecipeTag::new("Danish", "danish", None),
    RecipeTag::new("Dessert", "dessert", Some("🧁")),
    RecipeTag::new("Digestivo", "digestivo", None),
    RecipeTag::new("Dominican", "dominican", Some("🇩🇴")),
    RecipeTag::new("Dough", "dough", None),
    RecipeTag::new("Dressing", "dressing", None),
    RecipeTag::new("Drinks", "drinks", Some("🥤")),
    RecipeTag::new("Duck", "duck", Some("🦆")),
    RecipeTag::new("Dumpling", "dumpling", Some("🥟")),
    RecipeTag::new("Dutch", "dutch", Some("🇳🇱")),
    RecipeTag::new("Easter", "easter", Some("🐰")),
    RecipeTag::new("Easy", "easy", Some("😌")),
    RecipeTag::new("Eggs", "eggs", Some("🥚")),
    RecipeTag::new("English", "english", Some("🏴󠁧󠁢󠁥󠁮󠁧󠁿")),
    RecipeTag::new("Fasting", "fasting", None),
    RecipeTag::new("Feta", "feta", Some("🧀")),
    RecipeTag::new("Filipino", "filipino", Some("🇵🇭")),
    RecipeTag::new("Fish", "fish", Some("🐟")),
    RecipeTag::new("French", "french", Some("🇫🇷")),
    RecipeTag::new("Frozen", "frozen", Some("🥶")),
    RecipeTag::new("Fruit", "fruit", Some("🍇")),
    RecipeTag::new("Fry", "fry", None),
    RecipeTag::new("Galician", "galician", None),
    RecipeTag::new("Garlic", "garlic", Some("🧄")),
    RecipeTag::new("German", "german", Some("🇩🇪")),
    RecipeTag::new("Greek", "greek", Some("🇬🇷")),
    RecipeTag::new("Ham", "ham", Some("🍖")),
    RecipeTag::new("Hungarian", "hungarian", Some("🇭🇺")),
    RecipeTag::new("Indian", "indian", Some("🇮🇳")),
    RecipeTag::new("Irish", "irish", Some("☘️")),
    RecipeTag::new("Israeli", "israeli", Some("🇮🇱")),
    RecipeTag::new("Italian", "italian", Some("🇮🇹")),
    RecipeTag::new("Jam", "jam", None),
    RecipeTag::new("Japanese", "japanese", Some("🇯🇵")),
    RecipeTag::new("Keto", "keto", None),
    RecipeTag::new("Lamb", "lamb", Some("🐑")),
    RecipeTag::new("Layered", "layered", None),
    RecipeTag::new("Lebanese", "lebanese", Some("🇱🇧")),
    RecipeTag::new("Lemons", "lemons", Some("🍋")),
    RecipeTag::new("Lentil", "lentil", None),
    RecipeTag::new("Liquor", "liquor", Some("🥃")),
    RecipeTag::new("Liver", "liver", Some("🍖")),
    RecipeTag::new("Lunch", "lunch", Some("🍴")),
    RecipeTag::new("Meat", "meat", Some("🥩")),
    RecipeTag::new("Mediterranean", "mediterranean", Some("🏖️")),
    RecipeTag::new("Mexican", "mexican", Some("🇲🇽")),
    RecipeTag::new("Middle-Eastern", "middle-eastern", None),
    RecipeTag::new("Milk", "milk", Some("🥛")),
    RecipeTag::new("Mushrooms", "mushrooms", Some("🍄")),
    RecipeTag::new("Mutton", "mutton", Some("🐑")),
    RecipeTag::new("Noodles", "noodles", Some("🍜")),
    RecipeTag::new("Oven", "oven", Some("🔥")),
    RecipeTag::new("Palestinian", "palestinian", None),
    RecipeTag::new("Pancake", "pancake", Some("🥞")),
    RecipeTag::new("Pasta", "pasta", Some("🍝")),
    RecipeTag::new("Pastry", "pastry", Some("🥐")),
    RecipeTag::new("Pate", "pate", None),
    RecipeTag::new("Peppers", "peppers", Some("🌶️")),
    RecipeTag::new("Peruvian", "peruvian", Some("🇵🇪")),
    RecipeTag::new("Pie", "pie", Some("🥧")),
    RecipeTag::new("Pizza", "pizza", Some("🍕")),
    RecipeTag::new("Polish", "polish", Some("🇵🇱")),
    RecipeTag::new("Pork", "pork", Some("🐖")),
    RecipeTag::new("Portuguese", "portuguese", Some("🇵🇹")),
    RecipeTag::new("Potato", "potato", Some("🥔")),
    RecipeTag::new("Pub", "pub", None),
    RecipeTag::new("Quebec", "quebec", Some("🍁")),
    RecipeTag::new("Quick", "quick", Some("🌭")),
    RecipeTag::new("Raw", "raw", None),
    RecipeTag::new("Rice", "rice", Some("🍚")),
    RecipeTag::new("Roast", "roast", Some("🍖")),
    RecipeTag::new("Romanian", "romanian", Some("🇷🇴")),
    RecipeTag::new("Russian", "russian", Some("🇷🇺")),
    RecipeTag::new("Salad", "salad", Some("🥗")),
    RecipeTag::new("Sandwich", "sandwich", Some("🥪")),
    RecipeTag::new("Sauce", "sauce", Some("🍲")),
    RecipeTag::new("Sausage", "sausage", Some("🌭")),
    RecipeTag::new("Seafood", "seafood", Some("🦐")),
    RecipeTag::new("Shaken", "shaken", Some("🫨")),
    RecipeTag::new("Shrimp", "shrimp", Some("🦐")),
    RecipeTag::new("Side", "side", None),
    RecipeTag::new("Slowcooked", "slowcooked", Some("⏲️")),
    RecipeTag::new("Snack", "snack", Some("🍿")),
    RecipeTag::new("Soup", "soup", Some("🍲")),
    RecipeTag::new("Sourdough", "sourdough", Some("🍞")),
    RecipeTag::new("Southern", "southern", None),
    RecipeTag::new("Southwest", "southwest", None),
    RecipeTag::new("Spanish", "spanish", Some("🇪🇸")),
    RecipeTag::new("Spice", "spice", Some("🌶️")),
    RecipeTag::new("Spicy", "spicy", Some("🌶️")),
    RecipeTag::new("Spinach", "spinach", Some("🍃")),
    RecipeTag::new("Spread", "spread", None),
    RecipeTag::new("Squash", "squash", None),
    RecipeTag::new("Steak", "steak", Some("🥩")),
    RecipeTag::new("Stew", "stew", Some("🍲")),
    RecipeTag::new("Stirred", "stirred", Some("🥄")),
    RecipeTag::new("Stock", "stock", Some("🍲")),
    RecipeTag::new("Supper", "supper", Some("🍽️")),
    RecipeTag::new("Swedish", "swedish", Some("🇸🇪")),
    RecipeTag::new("Sweet", "sweet", Some("🍬")),
    RecipeTag::new("Swiss", "swiss", Some("🇨🇭")),
    RecipeTag::new("Syrup", "syrup", Some("🍯")),
    RecipeTag::new("Thai", "thai", Some("🇹🇭")),
    RecipeTag::new("Tofu", "tofu", Some("🥢")),
    RecipeTag::new("Tomato", "tomato", Some("🍅")),
    RecipeTag::new("Tortilla", "tortilla", Some("🌮")),
    RecipeTag::new("Traditional", "traditional", None),
    RecipeTag::new("Traybake", "traybake", None),
    RecipeTag::new("Tuna", "tuna", Some("🐟")),
    RecipeTag::new("Tunisian", "tunisian", Some("🇹🇳")),
    RecipeTag::new("Turkey", "turkey", Some("🦃")),
    RecipeTag::new("Turkish", "turkish", Some("🇹🇷")),
    RecipeTag::new("Ukrainian", "ukrainian", Some("🇺🇦")),
    RecipeTag::new("Uzbek", "uzbek", Some("🇺🇿")),
    RecipeTag::new("Veal", "veal", Some("🐄")),
    RecipeTag::new("Vegetables", "vegetables", Some("🥦")),
    RecipeTag::new("Vegan", "vegan", Some("🌱")),
    RecipeTag::new("Raw Vegan", "raw-vegan", None),
    RecipeTag::new("Vietnamese", "vietnamese", Some("🇻🇳")),
    RecipeTag::new("Wholemeal", "wholemeal", None),
    RecipeTag::new("Wine", "wine", Some("🍷")),
    RecipeTag::new("Yucatecan", "yucatecan", Some("🇲🇽")),
    RecipeTag::new("Healthy", "healthy", Some("🍏")),
    RecipeTag::new("Gluten Free", "gluten-free", Some("🥗")),
];

/// A curated section of tags for the explore page
#[derive(Clone, Debug)]
pub struct TagSection {
    pub emoji: &'static str,
    pub title: &'static str,
    pub tags: &'static [&'static str],
}

/// Curated tag sections for the /recipes explore page
pub static CURATED_TAG_SECTIONS: &[TagSection] = &[
    TagSection {
        emoji: "🍽️",
        title: "Why are you cooking?",
        tags: &["Easy", "Quick", "Breakfast", "Lunch", "Supper", "Dessert", "Snack", "Drinks"],
    },
    TagSection {
        emoji: "🌍",
        title: "Explore by culture",
        tags: &[
            "American", "Asian", "Chinese", "French", "German", "Greek", "Indian", "Italian",
            "Japanese", "Mexican", "Spanish", "Thai", "Turkish", "Vietnamese", "Mediterranean",
            "Middle-Eastern", "Brazilian", "Filipino", "Lebanese",
        ],
    },
    TagSection {
        emoji: "🥩",
        title: "Proteins",
        tags: &["Beef", "Chicken", "Fish", "Lamb", "Pork", "Seafood", "Steak", "Turkey", "Duck", "Eggs", "Tofu"],
    },
    TagSection {
        emoji: "🥕",
        title: "Ingredients",
        tags: &[
            "Apple", "Beans", "Bread", "Cheese", "Chocolate", "Coconut", "Corn", "Cream",
            "Fruit", "Garlic", "Mushrooms", "Noodles", "Pasta", "Peppers", "Potato", "Rice",
            "Spinach", "Tomato", "Vegetables",
        ],
    },
    TagSection {
        emoji: "🍳",
        title: "Meals",
        tags: &["Pizza", "Pasta", "Soup", "Salad", "Sandwich", "Breakfast", "Lunch", "Supper"],
    },
    TagSection {
        emoji: "🔥",
        title: "Methods",
        tags: &["Baked", "Fry", "Oven", "Roast", "Slowcooked"],
    },
    TagSection {
        emoji: "🥗",
        title: "Lifestyle",
        tags: &["Vegan", "Keto", "Healthy", "Gluten Free"],
    },
    TagSection {
        emoji: "🌶️",
        title: "Flavor",
        tags: &["Spicy", "Sweet", "Curry"],
    },
];

/// Tag aliases for normalization (maps variations to canonical tag names)
pub static TAG_ALIASES: &[(&str, &str)] = &[
    ("Spice", "Spicy"),
    ("Spices", "Spicy"),
    ("Hot", "Spicy"),
    ("Vegetarian", "Vegan"),
    ("Gluten-Free", "Gluten Free"),
    ("GlutenFree", "Gluten Free"),
    ("Middle Eastern", "Middle-Eastern"),
    ("MiddleEastern", "Middle-Eastern"),
];

/// Find a tag by name (case-insensitive)
pub fn find_tag(name: &str) -> Option<&'static RecipeTag> {
    let name_lower = name.to_lowercase();
    RECIPE_TAGS.iter().find(|t| t.name.to_lowercase() == name_lower)
}

/// Find a tag by slug
pub fn find_tag_by_slug(slug: &str) -> Option<&'static RecipeTag> {
    RECIPE_TAGS.iter().find(|t| t.slug == slug)
}

/// Normalize a tag name using aliases
pub fn normalize_tag(name: &str) -> &str {
    TAG_ALIASES
        .iter()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(name))
        .map(|(_, canonical)| *canonical)
        .unwrap_or(name)
}

/// Get tags matching a search query (case-insensitive prefix match)
pub fn search_tags(query: &str) -> Vec<&'static RecipeTag> {
    if query.is_empty() {
        return Vec::new();
    }
    let query_lower = query.to_lowercase();
    RECIPE_TAGS
        .iter()
        .filter(|t| t.name.to_lowercase().starts_with(&query_lower))
        .collect()
}

/// Get the curated tags for a specific section by title
pub fn get_section_tags(section_title: &str) -> Option<Vec<&'static RecipeTag>> {
    CURATED_TAG_SECTIONS
        .iter()
        .find(|s| s.title == section_title)
        .map(|section| {
            section.tags
                .iter()
                .filter_map(|name| find_tag(name))
                .collect()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_count() {
        assert_eq!(RECIPE_TAGS.len(), 163);
    }

    #[test]
    fn test_find_tag() {
        let italian = find_tag("Italian");
        assert!(italian.is_some());
        assert_eq!(italian.unwrap().emoji, Some("🇮🇹"));
    }

    #[test]
    fn test_nostr_tag() {
        let italian = find_tag("Italian").unwrap();
        assert_eq!(italian.nostr_tag(), "nostrcooking-italian");
    }

    #[test]
    fn test_normalize_tag() {
        assert_eq!(normalize_tag("Spice"), "Spicy");
        assert_eq!(normalize_tag("Gluten-Free"), "Gluten Free");
        assert_eq!(normalize_tag("Italian"), "Italian"); // No alias
    }

    #[test]
    fn test_search_tags() {
        let results = search_tags("It");
        assert!(results.iter().any(|t| t.name == "Italian"));
    }
}
