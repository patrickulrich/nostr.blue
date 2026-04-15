use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemCategory {
    #[default]
    Food,
    Toy,
    Medicine,
    Hygiene,
    Accessory,
}

impl ItemCategory {
    pub fn label(&self) -> &'static str {
        match self {
            ItemCategory::Food => "Food",
            ItemCategory::Toy => "Toys",
            ItemCategory::Medicine => "Medicine",
            ItemCategory::Hygiene => "Hygiene",
            ItemCategory::Accessory => "Accessories",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            ItemCategory::Food => "\u{1F354}",
            ItemCategory::Toy => "\u{1F3AE}",
            ItemCategory::Medicine => "\u{1F48A}",
            ItemCategory::Hygiene => "\u{2728}",
            ItemCategory::Accessory => "\u{1F452}",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ItemCategory::Food => "food",
            ItemCategory::Toy => "toy",
            ItemCategory::Medicine => "medicine",
            ItemCategory::Hygiene => "hygiene",
            ItemCategory::Accessory => "accessory",
        }
    }

    #[allow(dead_code)]
    pub fn all() -> &'static [ItemCategory] {
        &[
            ItemCategory::Food,
            ItemCategory::Toy,
            ItemCategory::Medicine,
            ItemCategory::Hygiene,
            ItemCategory::Accessory,
        ]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShopItem {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    pub category: ItemCategory,
    pub price: u64,
    pub stat_changes: Vec<(&'static str, f64)>,
    pub description: &'static str,
}

impl ShopItem {
    pub fn stat_summary(&self) -> String {
        self.stat_changes
            .iter()
            .map(|(stat, delta)| {
                let stat_char = stat
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .next()
                    .unwrap_or('?');
                if *delta >= 0.0 {
                    format!("{}+{:.0}", stat_char, delta)
                } else {
                    format!("{}{:.0}", stat_char, delta)
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn has_egg_effects(&self) -> bool {
        self.stat_changes
            .iter()
            .any(|(s, _)| *s == "egg_temperature" || *s == "shell_integrity")
    }
}

pub fn all_shop_items() -> Vec<ShopItem> {
    vec![
        ShopItem {
            id: "food_apple",
            name: "Apple",
            icon: "\u{1F34E}",
            category: ItemCategory::Food,
            price: 10,
            stat_changes: vec![("hunger", 15.0), ("hygiene", -2.0), ("energy", 5.0)],
            description: "A crisp red apple",
        },
        ShopItem {
            id: "food_burger",
            name: "Burger",
            icon: "\u{1F354}",
            category: ItemCategory::Food,
            price: 25,
            stat_changes: vec![
                ("hunger", 40.0),
                ("happiness", 10.0),
                ("hygiene", -8.0),
                ("energy", 8.0),
            ],
            description: "A juicy burger",
        },
        ShopItem {
            id: "food_cake",
            name: "Cake",
            icon: "\u{1F382}",
            category: ItemCategory::Food,
            price: 50,
            stat_changes: vec![
                ("hunger", 20.0),
                ("happiness", 30.0),
                ("hygiene", -10.0),
                ("energy", 10.0),
            ],
            description: "A special birthday cake",
        },
        ShopItem {
            id: "food_pizza",
            name: "Pizza",
            icon: "\u{1F355}",
            category: ItemCategory::Food,
            price: 35,
            stat_changes: vec![
                ("hunger", 35.0),
                ("happiness", 15.0),
                ("hygiene", -9.0),
                ("energy", 10.0),
            ],
            description: "Delicious pizza slice",
        },
        ShopItem {
            id: "food_sushi",
            name: "Sushi",
            icon: "\u{1F363}",
            category: ItemCategory::Food,
            price: 45,
            stat_changes: vec![
                ("hunger", 30.0),
                ("health", 10.0),
                ("hygiene", -6.0),
                ("energy", 7.0),
            ],
            description: "Fresh healthy sushi",
        },
        ShopItem {
            id: "toy_ball",
            name: "Ball",
            icon: "\u{26BD}",
            category: ItemCategory::Toy,
            price: 30,
            stat_changes: vec![("happiness", 25.0), ("energy", -10.0), ("hygiene", -5.0)],
            description: "A bouncy ball",
        },
        ShopItem {
            id: "toy_teddy",
            name: "Teddy Bear",
            icon: "\u{1F9F8}",
            category: ItemCategory::Toy,
            price: 60,
            stat_changes: vec![("happiness", 40.0), ("energy", -15.0)],
            description: "A cuddly teddy bear",
        },
        ShopItem {
            id: "toy_blocks",
            name: "Building Blocks",
            icon: "\u{1F9F1}",
            category: ItemCategory::Toy,
            price: 40,
            stat_changes: vec![("happiness", 30.0), ("energy", -10.0)],
            description: "Colorful building blocks",
        },
        ShopItem {
            id: "med_vitamins",
            name: "Vitamins",
            icon: "\u{1F48A}",
            category: ItemCategory::Medicine,
            price: 40,
            stat_changes: vec![("health", 20.0)],
            description: "Essential vitamins",
        },
        ShopItem {
            id: "med_super",
            name: "Super Medicine",
            icon: "\u{1F489}",
            category: ItemCategory::Medicine,
            price: 100,
            stat_changes: vec![("health", 50.0), ("energy", 20.0), ("happiness", -10.0)],
            description: "Powerful healing medicine",
        },
        ShopItem {
            id: "med_bandage",
            name: "Bandage",
            icon: "\u{1FA79}",
            category: ItemCategory::Medicine,
            price: 20,
            stat_changes: vec![("health", 15.0)],
            description: "A healing bandage",
        },
        ShopItem {
            id: "med_elixir",
            name: "Health Elixir",
            icon: "\u{2697}\u{FE0F}",
            category: ItemCategory::Medicine,
            price: 150,
            stat_changes: vec![("health", 80.0), ("happiness", 20.0), ("energy", 10.0)],
            description: "A powerful healing elixir",
        },
        ShopItem {
            id: "med_shell_repair",
            name: "Shell Repair Kit",
            icon: "\u{1F95A}",
            category: ItemCategory::Medicine,
            price: 60,
            stat_changes: vec![("health", 30.0)],
            description: "Repairs damaged shells",
        },
        ShopItem {
            id: "med_calcium",
            name: "Calcium Supplement",
            icon: "\u{1F9B4}",
            category: ItemCategory::Medicine,
            price: 35,
            stat_changes: vec![("health", 35.0)],
            description: "Strengthens bones and shell",
        },
        ShopItem {
            id: "hyg_soap",
            name: "Soap",
            icon: "\u{1F9FC}",
            category: ItemCategory::Hygiene,
            price: 15,
            stat_changes: vec![("hygiene", 30.0)],
            description: "A bar of soap",
        },
        ShopItem {
            id: "hyg_shampoo",
            name: "Shampoo",
            icon: "\u{1F9F4}",
            category: ItemCategory::Hygiene,
            price: 25,
            stat_changes: vec![("hygiene", 50.0), ("happiness", 10.0)],
            description: "Squeaky clean shampoo",
        },
        ShopItem {
            id: "hyg_bubble",
            name: "Bubble Bath",
            icon: "\u{1F6C0}",
            category: ItemCategory::Hygiene,
            price: 40,
            stat_changes: vec![("hygiene", 60.0), ("happiness", 20.0)],
            description: "A luxurious bubble bath",
        },
        ShopItem {
            id: "hyg_towel",
            name: "Soft Towel",
            icon: "\u{1F3D6}\u{FE0F}",
            category: ItemCategory::Hygiene,
            price: 20,
            stat_changes: vec![("hygiene", 25.0), ("happiness", 5.0)],
            description: "A fluffy soft towel",
        },
        ShopItem {
            id: "acc_hat",
            name: "Party Hat",
            icon: "\u{1F3A9}",
            category: ItemCategory::Accessory,
            price: 75,
            stat_changes: vec![],
            description: "A fancy party hat",
        },
        ShopItem {
            id: "acc_glasses",
            name: "Cool Glasses",
            icon: "\u{1F576}\u{FE0F}",
            category: ItemCategory::Accessory,
            price: 60,
            stat_changes: vec![],
            description: "Stylish sunglasses",
        },
        ShopItem {
            id: "acc_bow",
            name: "Bow Tie",
            icon: "\u{1F380}",
            category: ItemCategory::Accessory,
            price: 50,
            stat_changes: vec![],
            description: "A dapper bow tie",
        },
        ShopItem {
            id: "acc_crown",
            name: "Crown",
            icon: "\u{1F451}",
            category: ItemCategory::Accessory,
            price: 100,
            stat_changes: vec![],
            description: "A royal crown",
        },
    ]
}

pub fn find_item(id: &str) -> Option<ShopItem> {
    all_shop_items().into_iter().find(|i| i.id == id)
}

pub fn items_by_category(category: ItemCategory) -> Vec<ShopItem> {
    all_shop_items()
        .into_iter()
        .filter(|i| i.category == category)
        .collect()
}

pub fn shop_item_count() -> usize {
    all_shop_items().len()
}
