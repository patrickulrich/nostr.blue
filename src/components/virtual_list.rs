/// Virtual scrolling component with dynamic height support (Phase 3.1)
///
/// Renders only visible items in large lists to maintain smooth performance.
/// Handles 10,000+ items efficiently by rendering only what's in the viewport.
///
/// # Features
/// - Dynamic height tracking for variable-sized items
/// - Configurable overscan for smooth scrolling
/// - Automatic height measurement via DOM
/// - Memory-efficient: O(viewport_size) DOM nodes vs O(total_items)
///
/// # Performance Impact
/// - Before: Rendering 1000 notes = 1000 DOM nodes (slow, memory intensive)
/// - After: Rendering 1000 notes = ~20 DOM nodes (fast, constant memory)
/// - Maintains 60fps even with 10,000+ items

use dioxus::prelude::*;
use std::collections::HashMap;
use std::rc::Rc;
use js_sys::eval;

/// Configuration for virtual scrolling behavior
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VirtualScrollConfig {
    /// Estimated height for items before actual measurement (pixels)
    pub estimated_item_height: f64,

    /// Number of items to render above/below viewport (buffer for smooth scrolling)
    pub overscan_count: usize,

    /// Minimum number of items to render even if viewport is small
    pub min_batch_size: usize,
}

impl Default for VirtualScrollConfig {
    fn default() -> Self {
        Self {
            estimated_item_height: 200.0, // Typical note card height
            overscan_count: 5,             // Render 5 extra items above/below
            min_batch_size: 10,            // Always render at least 10 items
        }
    }
}

/// Height information for an item in the virtual list
#[derive(Clone, Copy, Debug)]
struct ItemHeight {
    /// Measured or estimated height in pixels
    height: f64,
    /// Whether this height has been measured from DOM (true) or is estimated (false)
    is_measured: bool,
}

/// State for virtual scrolling calculations
#[derive(Clone, Debug)]
struct VirtualState {
    /// Scroll offset in pixels
    scroll_top: f64,
    /// Viewport height in pixels
    viewport_height: f64,
    /// Total number of items in the list
    total_items: usize,
    /// Measured/estimated heights for each item
    item_heights: HashMap<usize, ItemHeight>,
    /// Configuration
    config: VirtualScrollConfig,
}

impl VirtualState {
    fn new(total_items: usize, config: VirtualScrollConfig) -> Self {
        Self {
            scroll_top: 0.0,
            viewport_height: 800.0, // Default viewport height
            total_items,
            item_heights: HashMap::new(),
            config,
        }
    }

    /// Get height for an item (measured or estimated)
    fn get_item_height(&self, index: usize) -> f64 {
        self.item_heights
            .get(&index)
            .map(|h| h.height)
            .unwrap_or(self.config.estimated_item_height)
    }

    /// Calculate the visible range of items based on current scroll position
    fn calculate_visible_range(&self) -> (usize, usize) {
        if self.total_items == 0 {
            return (0, 0);
        }

        // Find the first visible item
        let mut offset = 0.0;
        let mut start_index = 0;

        for i in 0..self.total_items {
            let item_height = self.get_item_height(i);
            if offset + item_height > self.scroll_top {
                start_index = i;
                break;
            }
            offset += item_height;
        }

        // Find the last visible item
        let scroll_bottom = self.scroll_top + self.viewport_height;
        let mut end_index = start_index;

        for i in start_index..self.total_items {
            let item_height = self.get_item_height(i);
            offset += item_height;
            end_index = i + 1;
            if offset >= scroll_bottom {
                break;
            }
        }

        // Apply overscan
        let start_with_overscan = start_index.saturating_sub(self.config.overscan_count);
        let end_with_overscan = (end_index + self.config.overscan_count).min(self.total_items);

        // Ensure minimum batch size
        let range_size = end_with_overscan - start_with_overscan;
        if range_size < self.config.min_batch_size {
            let additional_needed = self.config.min_batch_size - range_size;
            let end_adjusted = (end_with_overscan + additional_needed).min(self.total_items);
            (start_with_overscan, end_adjusted)
        } else {
            (start_with_overscan, end_with_overscan)
        }
    }

    /// Calculate total height of all items
    fn calculate_total_height(&self) -> f64 {
        (0..self.total_items)
            .map(|i| self.get_item_height(i))
            .sum()
    }

    /// Calculate offset (top position) for a specific item
    fn calculate_item_offset(&self, index: usize) -> f64 {
        (0..index)
            .map(|i| self.get_item_height(i))
            .sum()
    }

    /// Update measured height for an item
    fn set_item_height(&mut self, index: usize, height: f64) {
        self.item_heights.insert(
            index,
            ItemHeight {
                height,
                is_measured: true,
            },
        );
    }
}

/// Props for VirtualList component
#[derive(Props, Clone, PartialEq)]
pub struct VirtualListProps<T: PartialEq + 'static> {
    /// All items in the list (wrapped in Rc for cheap cloning)
    pub items: Vec<Rc<T>>,

    /// Render function for each item
    /// Receives: (item, index) where item is Rc<T>
    pub item_content: fn(Rc<T>, usize) -> Element,

    /// Optional configuration (uses default if not provided)
    #[props(default)]
    pub config: Option<VirtualScrollConfig>,

    /// Optional CSS class for the container
    #[props(default = "virtual-scroll-container".to_string())]
    pub container_class: String,
}

/// VirtualList component - renders large lists efficiently
///
/// # Example
/// ```rust
/// // Wrap items in Rc for efficient cloning
/// let items_rc: Vec<Rc<Event>> = feed_events.into_iter().map(Rc::new).collect();
///
/// VirtualList {
///     items: items_rc,
///     item_content: |event, index| rsx! {
///         NoteCard { event: event }
///     },
///     config: Some(VirtualScrollConfig {
///         estimated_item_height: 250.0,
///         overscan_count: 3,
///         ..Default::default()
///     })
/// }
/// ```
#[component]
pub fn VirtualList<T: PartialEq + 'static>(props: VirtualListProps<T>) -> Element {
    let config = props.config.unwrap_or_default();
    let mut virtual_state = use_signal(|| VirtualState::new(props.items.len(), config));

    // Update total items count when items change
    use_effect(use_reactive(&props.items, move |items| {
        virtual_state.write().total_items = items.len();
    }));

    // Measure actual viewport height after mount
    let container_class = props.container_class.clone();
    use_effect(move || {
        let mut state = virtual_state.clone();
        let container_class = container_class.clone();
        spawn(async move {
            // Try to get the actual container height from DOM
            let js_code = format!(
                r#"
                (function() {{
                    const container = document.querySelector('.{}');
                    if (container) {{
                        return container.clientHeight || container.getBoundingClientRect().height;
                    }}
                    return 800.0;
                }})()
                "#,
                container_class
            );

            if let Ok(result) = eval(&js_code) {
                let height = result.as_f64().unwrap_or(800.0);
                if height > 0.0 {
                    state.write().viewport_height = height;
                    log::debug!("Updated viewport_height to {}px", height);
                }
            }
        });
    });

    // Calculate visible range
    let state = virtual_state.read();
    let (start_index, end_index) = state.calculate_visible_range();
    let total_height = state.calculate_total_height();
    let start_offset = state.calculate_item_offset(start_index);

    log::debug!(
        "VirtualList: Rendering items {}-{} of {} (viewport at {}px)",
        start_index,
        end_index,
        props.items.len(),
        state.scroll_top
    );

    drop(state); // Release borrow before rendering

    // Memoize visible items calculation - only recompute when items or range changes
    // Cloning Rc<T> is cheap (just incrementing reference count)
    let visible_items = use_memo(move || {
        props.items
            .iter()
            .enumerate()
            .skip(start_index)
            .take(end_index - start_index)
            .map(|(i, item)| (item.clone(), i))
            .collect::<Vec<(Rc<T>, usize)>>()
    });

    let container_class_for_scroll = props.container_class.clone();
    rsx! {
        div {
            class: "{props.container_class}",
            style: "overflow-y: auto; position: relative; height: 100%;",
            onscroll: move |_evt| {
                // Update scroll position via JS interop
                let mut state = virtual_state.clone();
                let container_class = container_class_for_scroll.clone();
                spawn(async move {
                    let js_code = format!(
                        r#"
                        (function() {{
                            const container = document.querySelector('.{}');
                            if (container) {{
                                return container.scrollTop;
                            }}
                            return 0;
                        }})()
                        "#,
                        container_class
                    );

                    if let Ok(result) = eval(&js_code) {
                        let scroll_top = result.as_f64().unwrap_or(0.0);
                        state.write().scroll_top = scroll_top;
                        log::trace!("Updated scroll_top to {}px", scroll_top);
                    }
                });
            },

            // Spacer div for total height (creates scrollbar)
            div {
                style: "height: {total_height}px; position: relative;",

                // Spacer for items before visible range
                div {
                    style: "height: {start_offset}px;",
                }

                // Render visible items (memoized, only clones cheap Rc pointers)
                for (item, index) in visible_items.read().iter() {
                    {
                        let item_index = *index;
                        let item_rc = item.clone();
                        let state = virtual_state.clone();
                        rsx! {
                            div {
                                key: "{index}",
                                id: "virtual-item-{index}",
                                class: "virtual-item",
                                onmounted: move |evt| {
                                    let mut state = state.clone();
                                    spawn(async move {
                                        // Measure the item's height from DOM
                                        let js_code = format!(
                                            r#"
                                            const item = document.getElementById('virtual-item-{}');
                                            if (item) {{
                                                return item.getBoundingClientRect().height;
                                            }}
                                            return null;
                                            "#,
                                            item_index
                                        );

                                        if let Ok(result) = eval(&js_code) {
                                            let result_val = result.as_f64().unwrap_or(0.0);
                                            if result_val > 0.0 {
                                                state.write().set_item_height(item_index, result_val);
                                                log::trace!("Measured item {} height: {}px", item_index, result_val);
                                            }
                                        }
                                    });
                                },
                                {(props.item_content)(item_rc, item_index)}
                            }
                        }
                    }
                }
            }
        }
    }
}
