//! ProductGrid component - responsive grid layout for products
use super::{ProductCard, ProductCardSkeleton};
use crate::utils::nip99::Product;
use dioxus::prelude::*;
#[derive(Props, Clone, PartialEq)]
pub struct ProductGridProps {
    pub products: Vec<Product>,
    #[props(default = false)]
    pub loading: bool,
}
/// Responsive product grid
#[component]
pub fn ProductGrid(props: ProductGridProps) -> Element {
    rsx! {
        div { class: "grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4",
            if props.loading {
                for i in 0..8 {
                    ProductCardSkeleton { key: "{i}" }
                }
            } else if props.products.is_empty() {
                div { class: "col-span-full",
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "📭" }
                        h3 { class: "text-lg font-semibold mb-2", "No Products Found" }
                        p { class: "text-muted-foreground", "Check back later for new listings" }
                    }
                }
            } else {
                for product in props.products.iter() {
                    ProductCard { key: "{product.naddr}", product: product.clone() }
                }
            }
        }
    }
}
