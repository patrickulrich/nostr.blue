pub mod blossom_store;
pub mod gif_store;
pub mod lightbox_store;
pub mod nip96_store;

pub use lightbox_store::{
    close_lightbox, next_image, open_lightbox, prev_image, set_index, LightboxImage,
    LIGHTBOX_STATE,
};
