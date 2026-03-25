use dioxus::prelude::*;
use dioxus_primitives::dialog::{
    self, DialogCtx, DialogDescriptionProps, DialogRootProps, DialogTitleProps,
};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SheetSide {
    Top,
    #[default]
    Right,
    Bottom,
    Left,
}

impl SheetSide {
    fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Right => "right",
            Self::Bottom => "bottom",
            Self::Left => "left",
        }
    }
}

#[component]
pub fn Sheet(props: DialogRootProps) -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: asset!("./sheet.css") }
        dialog::DialogRoot {
            class: "sheet-root",
            id: props.id,
            is_modal: props.is_modal,
            open: props.open,
            default_open: props.default_open,
            on_open_change: props.on_open_change,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn SheetContent(
    #[props(default = ReadSignal::new(Signal::new(None)))] id: ReadSignal<Option<String>>,
    #[props(default)] side: SheetSide,
    #[props(default)] class: Option<String>,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let class = class
        .map(|existing| format!("sheet {existing}"))
        .unwrap_or_else(|| "sheet".to_string());

    rsx! {
        dialog::DialogContent {
            class,
            id,
            "data-side": side.as_str(),
            attributes,
            {children}
        }
    }
}

#[component]
pub fn SheetHeader(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "sheet-header", ..attributes, {children} }
    }
}

#[component]
pub fn SheetFooter(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "sheet-footer", ..attributes, {children} }
    }
}

#[component]
pub fn SheetTitle(props: DialogTitleProps) -> Element {
    rsx! {
        dialog::DialogTitle {
            class: "sheet-title",
            id: props.id,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn SheetDescription(props: DialogDescriptionProps) -> Element {
    rsx! {
        dialog::DialogDescription {
            class: "sheet-description",
            id: props.id,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn SheetClose(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    r#as: Option<Callback<Vec<Attribute>, Element>>,
    children: Element,
) -> Element {
    let ctx: DialogCtx = use_context();

    let mut attrs = attributes;
    attrs.push(Attribute::new(
        "onclick",
        dioxus::dioxus_core::AttributeValue::listener(move |_: Event<MouseData>| {
            ctx.set_open(false);
        }),
        None,
        false,
    ));

    if let Some(renderer) = r#as {
        renderer.call(attrs)
    } else {
        rsx! {
            button { ..attrs, {children} }
        }
    }
}
