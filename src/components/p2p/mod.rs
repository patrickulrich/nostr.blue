//! P2P trading components (NIP-69)
//!
//! This module contains components for peer-to-peer trading
//! including order cards, filters, and depth charts.

pub mod order_card;
pub mod status_badge;
pub mod order_filters;
pub mod depth_chart;

// Re-export main component types for convenience
pub use order_card::{P2POrderCard, P2POrderCardSkeleton};
pub use status_badge::{P2PStatusBadge, P2PTypeBadge, P2PLayerBadge, P2PNetworkBadge};
pub use order_filters::P2POrderFilters;
pub use depth_chart::{P2PDepthChart, P2PDepthChartSkeleton};
