/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use ipc_channel::ipc::IpcSharedMemory;
use piston_image::{self, DynamicImage, GenericImage};
use stb_image::image as stb_image2;
use util::vec::byte_swap;

pub use msg::constellation_msg::{Image, PixelFormat};

// FIXME: Images must not be copied every frame. Instead we should atomically
// reference count them.

// TODO(pcwalton): Speed up with SIMD, or better yet, find some way to not do this.
fn byte_swap_and_premultiply(data: &mut [u8]) {
    let length = data.len();
    for i in (0..length).step_by(4) {
        let r = data[i + 2];
        let g = data[i + 1];
        let b = data[i + 0];
        let a = data[i + 3];
        data[i + 0] = ((r as u32) * (a as u32) / 255) as u8;
        data[i + 1] = ((g as u32) * (a as u32) / 255) as u8;
        data[i + 2] = ((b as u32) * (a as u32) / 255) as u8;
    }
}

pub fn load_from_memory(buffer: &[u8]) -> Option<Image> {
    if buffer.is_empty() {
        return None;
    }

    if is_jpeg(buffer) {
        // For JPEG images, we use stb_image because piston_image does not yet support progressive
        // JPEG.

        // Can't remember why we do this. Maybe it's what cairo wants
        static FORCE_DEPTH: usize = 4;

        match stb_image2::load_from_memory_with_depth(buffer, FORCE_DEPTH, true) {
            stb_image2::LoadResult::ImageU8(mut image) => {
                assert!(image.depth == 4);
                // handle gif separately because the alpha-channel has to be premultiplied
                if is_gif(buffer) {
                    byte_swap_and_premultiply(&mut image.data);
                } else {
                    byte_swap(&mut image.data);
                }
                Some(Image {
                    width: image.width as u32,
                    height: image.height as u32,
                    format: PixelFormat::RGBA8,
                    bytes: IpcSharedMemory::from_bytes(&image.data[..]),
                })
            }
            stb_image2::LoadResult::ImageF32(_image) => {
                debug!("HDR images not implemented");
                None
            }
            stb_image2::LoadResult::Error(e) => {
                debug!("stb_image failed: {}", e);
                None
            }
        }
    } else {
        match piston_image::load_from_memory(buffer) {
            Ok(image) => {
                let mut rgba = match image {
                    DynamicImage::ImageRgba8(rgba) => rgba,
                    image => image.to_rgba()
                };
                byte_swap_and_premultiply(&mut *rgba);
                Some(Image {
                    width: rgba.width(),
                    height: rgba.height(),
                    format: PixelFormat::RGBA8,
                    bytes: IpcSharedMemory::from_bytes(&*rgba),
                })
            }
            Err(e) => {
                debug!("Image decoding error: {:?}", e);
                None
            }
        }
    }
}

fn is_gif(buffer: &[u8]) -> bool {
    match buffer {
        [b'G', b'I', b'F', b'8', n, b'a', ..] if n == b'7' || n == b'9' => true,
        _ => false
    }
}

fn is_jpeg(buffer: &[u8]) -> bool {
    buffer.starts_with(&[0xff, 0xd8, 0xff])
}
