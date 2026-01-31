//! P2P trading components (NIP-69)
//!
//! This module contains components for peer-to-peer trading
//! including order cards, filters, and depth charts.
pub mod depth_chart;
pub mod order_card;
pub mod order_filters;
pub mod status_badge;
pub use depth_chart::{P2PDepthChart, P2PDepthChartSkeleton};
pub use order_card::{P2POrderCard, P2POrderCardSkeleton};
pub use order_filters::P2POrderFilters;
pub use status_badge::{P2PLayerBadge, P2PNetworkBadge, P2PStatusBadge, P2PTypeBadge};
