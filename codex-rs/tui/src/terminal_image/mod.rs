//! Terminal image protocols used by coding transcripts.
mod image_protocol;
mod sixel;
pub(crate) use image_protocol::ImageProtocol;
pub(crate) use image_protocol::detect_image_support;
pub(crate) use image_protocol::kitty_delete_image;
pub(crate) use image_protocol::kitty_transmit_png_file_with_id;
pub(crate) use image_protocol::kitty_transmit_png_with_id;
pub(crate) use sixel::encode_rgba;
