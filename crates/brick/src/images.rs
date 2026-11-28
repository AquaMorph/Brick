//! Captured images decoded on background threads and kept as textures, so
//! large DSLR JPEGs never stall drawing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use eframe::egui;

/// Longest side of a thumbnail, in pixels.
const THUMBNAIL_SIDE: u32 = 256;
/// Longest side of a full size image. Larger images are scaled down to keep
/// texture memory in check; this is still well above screen resolution.
const FULL_SIDE: u32 = 4096;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Size {
    Thumbnail,
    Full,
}

enum Slot {
    Loading,
    Ready(egui::TextureHandle),
    Failed(String),
}

pub enum Image<'a> {
    Loading,
    Ready(&'a egui::TextureHandle),
    Failed(&'a str),
}

type Decoded = (PathBuf, Size, Result<egui::ColorImage, String>);

pub struct Images {
    thumbnails: HashMap<PathBuf, Slot>,
    /// Only one full size image is kept at a time.
    full: Option<(PathBuf, Slot)>,
    sender: Sender<Decoded>,
    receiver: Receiver<Decoded>,
}

impl Default for Images {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            thumbnails: HashMap::new(),
            full: None,
            sender,
            receiver,
        }
    }
}

impl Images {
    pub fn thumbnail(&mut self, ctx: &egui::Context, path: &Path) -> Image<'_> {
        self.receive(ctx);
        if !self.thumbnails.contains_key(path) {
            self.decode(ctx, path, Size::Thumbnail);
            self.thumbnails.insert(path.to_owned(), Slot::Loading);
        }
        image(self.thumbnails.get(path))
    }

    pub fn full(&mut self, ctx: &egui::Context, path: &Path) -> Image<'_> {
        self.receive(ctx);
        if self.full.as_ref().is_none_or(|(p, _)| p != path) {
            self.decode(ctx, path, Size::Full);
            self.full = Some((path.to_owned(), Slot::Loading));
        }
        image(self.full.as_ref().map(|(_, slot)| slot))
    }

    fn decode(&self, ctx: &egui::Context, path: &Path, size: Size) {
        let max_side = match size {
            Size::Thumbnail => THUMBNAIL_SIDE,
            Size::Full => FULL_SIDE.min(ctx.input(|i| i.max_texture_side) as u32),
        };
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        let path = path.to_owned();
        thread::spawn(move || {
            let result = load(&path, max_side);
            let _ = sender.send((path, size, result));
            ctx.request_repaint();
        });
    }

    fn receive(&mut self, ctx: &egui::Context) {
        for (path, size, result) in self.receiver.try_iter() {
            let slot = match result {
                Ok(image) => Slot::Ready(ctx.load_texture(
                    path.display().to_string(),
                    image,
                    egui::TextureOptions::LINEAR,
                )),
                Err(e) => Slot::Failed(e),
            };
            match size {
                Size::Thumbnail => {
                    if let Some(entry) = self.thumbnails.get_mut(&path) {
                        *entry = slot;
                    }
                }
                // A full image that is no longer wanted is dropped.
                Size::Full => {
                    if let Some((wanted, entry)) = &mut self.full
                        && *wanted == path
                    {
                        *entry = slot;
                    }
                }
            }
        }
    }
}

fn image(slot: Option<&Slot>) -> Image<'_> {
    match slot {
        None | Some(Slot::Loading) => Image::Loading,
        Some(Slot::Ready(texture)) => Image::Ready(texture),
        Some(Slot::Failed(e)) => Image::Failed(e),
    }
}

/// Decodes an image, scaled down so neither side is longer than `max_side`.
pub fn load(path: &Path, max_side: u32) -> Result<egui::ColorImage, String> {
    let mut image = image::open(path).map_err(|e| e.to_string())?;
    if image.width() > max_side || image.height() > max_side {
        image = image.thumbnail(max_side, max_side);
    }
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(size, &rgba))
}
