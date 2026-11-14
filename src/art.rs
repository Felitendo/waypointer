//! Hand pictures ready for the GPU: the hand and the stick cut apart and scaled to about the
//! size they are shown at, their soft shadows, and the thumbnails. Made on worker threads and
//! kept, so a frame never waits for a picture.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use image::RgbaImage;
use rune::Pixels;

use crate::hands::{self, Metrics, Picture};

pub struct Art {
    pub metrics: Metrics,
    pub hand: Pixels,
    pub rod: Pixels,
    pub hand_shadow: Pixels,
    pub rod_shadow: Pixels,
    /// The shadow pictures' room around the hand and stick, as a share of the picture's width.
    pub pad: f32,
}

fn pixels(img: &RgbaImage) -> Pixels {
    let p = Pixels::new();
    p.set(img.width(), img.height(), img.as_raw().clone());
    p
}

impl Art {
    fn make(id: &str, picture_px: u32) -> Option<Art> {
        let picture = Picture::load(id)?;
        let hand = picture.part(false, picture_px);
        let rod = picture.part(true, picture_px);
        // Shadows are made at a quarter of the size and drawn larger: they are soft anyway.
        let small = |img: &RgbaImage| image::imageops::resize(img, (img.width() / 4).max(1), (img.height() / 4).max(1), image::imageops::FilterType::Triangle);
        let pad = (hand.width() / 4 / 6).max(2);
        let blur = pad as f32 / 2.5;
        let hand_shadow = hands::silhouette(&small(&hand), pad, blur);
        let rod_shadow = hands::silhouette(&small(&rod), pad, blur);
        Some(Art {
            metrics: picture.metrics,
            pad: pad as f32 / (hand.width() / 4).max(1) as f32,
            hand: pixels(&hand),
            rod: pixels(&rod),
            hand_shadow: pixels(&hand_shadow),
            rod_shadow: pixels(&rod_shadow),
        })
    }
}

/// Pictures are scaled in steps, so a size change makes a new one only now and then.
pub fn picture_px(points: f32, scale: f32) -> u32 {
    ((points * scale / 64.0).ceil() as u32).max(1) * 64
}

#[derive(Default)]
pub struct Cache {
    art: Mutex<HashMap<(String, u32), Arc<Art>>>,
    thumbs: Mutex<HashMap<String, Pixels>>,
    loading: Mutex<HashSet<String>>,
}

impl Cache {
    /// The art for a pointer at this size, or `None` while it is made. `ready` runs on the
    /// worker once it is there.
    pub fn art(self: &Arc<Self>, id: &str, picture_px: u32, ready: impl FnOnce() + Send + 'static) -> Option<Arc<Art>> {
        let key = (id.to_string(), picture_px);
        if let Some(art) = self.art.lock().unwrap().get(&key) {
            return Some(art.clone());
        }
        let job = format!("art:{id}:{picture_px}");
        if !self.loading.lock().unwrap().insert(job.clone()) {
            return None;
        }
        let cache = self.clone();
        std::thread::spawn(move || {
            if let Some(art) = Art::make(&key.0, key.1) {
                let mut map = cache.art.lock().unwrap();
                // Keep the pointer in use and a few others.
                if map.len() > 6 {
                    map.clear();
                }
                map.insert(key, Arc::new(art));
            }
            cache.loading.lock().unwrap().remove(&job);
            ready();
        });
        None
    }

    pub fn thumb(&self, id: &str) -> Option<Pixels> {
        self.thumbs.lock().unwrap().get(id).cloned()
    }

    pub fn forget(&self, id: &str) {
        self.thumbs.lock().unwrap().remove(id);
        self.art.lock().unwrap().retain(|k, _| k.0 != id);
    }

    /// Makes the thumbnails that are missing, from the disk cache where it can.
    pub fn load_thumbs(self: &Arc<Self>, ids: Vec<String>, ready: impl Fn() + Send + 'static) {
        let cache = self.clone();
        std::thread::spawn(move || {
            for id in ids {
                if cache.thumbs.lock().unwrap().contains_key(&id) {
                    continue;
                }
                if let Some(img) = thumb_image(&id) {
                    cache.thumbs.lock().unwrap().insert(id, pixels(&img));
                }
            }
            ready();
        });
    }
}

fn thumb_cache_path(id: &str) -> Option<PathBuf> {
    let source = hands::picture_path(id)?;
    let meta = std::fs::metadata(&source).ok()?;
    let stamp = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some(dirs::cache_dir()?.join(format!("waypointer/thumbs/{id}-{stamp}-{}.png", meta.len())))
}

fn thumb_image(id: &str) -> Option<RgbaImage> {
    let cached = thumb_cache_path(id);
    if let Some(path) = &cached
        && let Ok(img) = image::open(path)
    {
        return Some(img.to_rgba8());
    }
    let img = Picture::load(id)?.thumbnail(128);
    if let Some(path) = cached {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = img.save(path);
    }
    Some(img)
}
