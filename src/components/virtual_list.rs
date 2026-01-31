//! Virtual scrolling component with dynamic height support (Phase 3.1)
//!
//! Renders only visible items in large lists to maintain smooth performance.
//! Handles 10,000+ items efficiently by rendering only what's in the viewport.
//!
//! # Features
//! - Dynamic height tracking for variable-sized items
//! - Configurable overscan for smooth scrolling
//! - Automatic height measurement via DOM
//! - Memory-efficient: O(viewport_size) DOM nodes vs O(total_items)
//!
//! # Performance Impact
//! - Before: Rendering 1000 notes = 1000 DOM nodes (slow, memory intensive)
//! - After: Rendering 1000 notes = ~20 DOM nodes (fast, constant memory)
//! - Maintains 60fps even with 10,000+ items
use dioxus::prelude::*;
use std::collections::HashMap;
use std::rc::Rc;
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
thread_local! {
    /// Track if a scroll update is pending (prevents flooding with rAF callbacks)
    static SCROLL_UPDATE_PENDING: RefCell<bool> = const { RefCell::new(false) };
}
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = requestAnimationFrame)]
    fn request_animation_frame(closure: &js_sys::Function);
}
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
            estimated_item_height: 200.0,
            overscan_count: 5,
            min_batch_size: 10,
        }
    }
}
/// Height information for an item in the virtual list
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
struct ItemHeight {
    /// Measured or estimated height in pixels
    height: f64,
    /// Whether this height has been measured from DOM (true) or is estimated (false)
    is_measured: bool,
}
/// State for virtual scrolling calculations
#[allow(dead_code)]
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
#[allow(dead_code)]
impl VirtualState {
    fn new(total_items: usize, config: VirtualScrollConfig) -> Self {
        Self {
            scroll_top: 0.0,
            viewport_height: 800.0,
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
        let start_with_overscan = start_index.saturating_sub(self.config.overscan_count);
        let end_with_overscan = (end_index + self.config.overscan_count)
            .min(self.total_items);
        let range_size = end_with_overscan - start_with_overscan;
        if range_size < self.config.min_batch_size {
            let additional_needed = self.config.min_batch_size - range_size;
            let end_adjusted = (end_with_overscan + additional_needed)
                .min(self.total_items);
            (start_with_overscan, end_adjusted)
        } else {
            (start_with_overscan, end_with_overscan)
        }
    }
    /// Calculate total height of all items
    fn calculate_total_height(&self) -> f64 {
        (0..self.total_items).map(|i| self.get_item_height(i)).sum()
    }
    /// Calculate offset (top position) for a specific item
    fn calculate_item_offset(&self, index: usize) -> f64 {
        (0..index).map(|i| self.get_item_height(i)).sum()
    }
    /// Update measured height for an item
    fn set_item_height(&mut self, index: usize, height: f64) {
        self.item_heights
            .insert(
                index,
                ItemHeight {
                    height,
                    is_measured: true,
                },
            );
    }
}
/// Props for VirtualList component
#[allow(unpredictable_function_pointer_comparisons)]
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
    #[cfg(target_arch = "wasm32")]
    let mut container_element = use_signal(|| None::<web_sys::HtmlElement>);
    use_effect(
        use_reactive(
            &props.items,
            move |items| {
                virtual_state.write().total_items = items.len();
            },
        ),
    );
    let state = virtual_state.read();
    let (start_index, end_index) = state.calculate_visible_range();
    let total_height = state.calculate_total_height();
    let start_offset = state.calculate_item_offset(start_index);
    log::debug!(
        "VirtualList: Rendering items {}-{} of {} (viewport at {}px)", start_index,
        end_index, props.items.len(), state.scroll_top
    );
    drop(state);
    let visible_items = use_memo(move || {
        props
            .items
            .iter()
            .enumerate()
            .skip(start_index)
            .take(end_index - start_index)
            .map(|(i, item)| (item.clone(), i))
            .collect::<Vec<(Rc<T>, usize)>>()
    });
    rsx! {
        div {
            class: "{props.container_class}",
            style: "overflow-y: auto; position: relative; height: 100%;",
            onmounted: move |_evt| {
                #[cfg(target_arch = "wasm32")]
                {
                    let element = _evt.data();
                    if let Some(html_element) = element.downcast::<web_sys::HtmlElement>() {
                        *container_element.write() = Some(html_element.clone());
                        let height = html_element.client_height() as f64;
                        if height > 0.0 {
                            virtual_state.write().viewport_height = height;
                            log::debug!("Updated viewport_height to {}px", height);
                        }
                    }
                }
            },
            onscroll: move |_evt| {
                #[cfg(target_arch = "wasm32")]
                {
                    let already_pending = SCROLL_UPDATE_PENDING
                        .with(|pending| {
                            let was_pending = *pending.borrow();
                            if !was_pending {
                                *pending.borrow_mut() = true;
                            }
                            was_pending
                        });
                    if already_pending {
                        return;
                    }
                    let mut state = virtual_state;
                    let container = container_element;
                    let closure = wasm_bindgen::closure::Closure::once_into_js(move || {
                        spawn(async move {
                            if let Some(html_element) = container.read().as_ref() {
                                let scroll_top = html_element.scroll_top() as f64;
                                state.write().scroll_top = scroll_top;
                                log::trace!("Updated scroll_top to {}px", scroll_top);
                            }
                            SCROLL_UPDATE_PENDING
                                .with(|pending| {
                                    *pending.borrow_mut() = false;
                                });
                        });
                    });
                    request_animation_frame(closure.unchecked_ref());
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    use std::sync::atomic::{AtomicU64, Ordering};
                    static LAST_SCROLL_UPDATE: AtomicU64 = AtomicU64::new(0);
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;
                    let last = LAST_SCROLL_UPDATE.load(Ordering::Relaxed);
                    if now - last < 16 {
                        return;
                    }
                    LAST_SCROLL_UPDATE.store(now, Ordering::Relaxed);
                    let mut state = virtual_state;
                    state.write().scroll_top = 0.0;
                    log::trace!("Updated scroll_top (non-WASM fallback)");
                }
            },
            div { style: "height: {total_height}px; position: relative;",
                div { style: "height: {start_offset}px;" }
                for (item , index) in visible_items.read().iter() {
                    {
                        let item_index = *index;
                        let item_rc = item.clone();
                        #[allow(unused_variables)]
                        let state = virtual_state;
                        rsx! {
                            div {
                                key: "{index}",
                                class: "virtual-item",
                                onmounted: move |_evt| {
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        let mut state = state;
                                        spawn(async move {
                                            let element = _evt.data();
                                            if let Some(html_element) = element.downcast::<web_sys::HtmlElement>() {
                                                let rect = html_element.get_bounding_client_rect();
                                                let height = rect.height();
                                                if height > 0.0 {
                                                    state.write().set_item_height(item_index, height);
                                                    log::trace!("Measured item {} height: {}px", item_index, height);
                                                }
                                            }
                                        });
                                    }
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
