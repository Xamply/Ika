//! Ready-to-display modpack catalogs. The open dialog owns a fixed snapshot;
//! refreshing the disk copy never replaces the user's current selection.
use super::modpacks::{self, DescriptionBlock, Details, Project, Provider, Release};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub const PAGE_SIZE: usize = 15;
const READY_PAGES: usize = 6;
const MAX_CACHE: u64 = 4 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct FirstDetails {
    description: String,
    gallery: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    gallery_thumbnails: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    body: Vec<DescriptionBlock>,
    releases: Vec<Release>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hero: Option<(String, String)>,
}

#[derive(Serialize, Deserialize)]
pub struct Catalog {
    pub projects: Vec<Project>,
    covers: HashMap<String, String>,
    first_details: Option<FirstDetails>,
}

impl Catalog {
    pub fn first_details(&self) -> Option<Details> {
        self.first_details.as_ref().map(|details| Details {
            description: details.description.clone(),
            gallery: details.gallery.clone(),
            gallery_thumbnails: details.gallery_thumbnails.clone(),
            body: details.body.clone(),
            versions: Ok(details.releases.clone()),
        })
    }

    pub fn page_images(&self, page: usize) -> Vec<(String, RgbaImage)> {
        self.page(page)
            .iter()
            .filter_map(|project| {
                let bytes = self.covers.get(&project.id)?;
                Some((project.id.clone(), decode_jpeg(bytes, 96 * 1024, 192)?))
            })
            .collect()
    }

    pub fn first_hero(&self) -> Option<(String, RgbaImage)> {
        let (url, bytes) = self.first_details.as_ref()?.hero.as_ref()?;
        Some((url.clone(), decode_jpeg(bytes, 384 * 1024, 720)?))
    }

    pub fn page(&self, page: usize) -> &[Project] {
        let offset = (page * PAGE_SIZE).min(self.projects.len());
        &self.projects[offset..(offset + PAGE_SIZE).min(self.projects.len())]
    }
}

fn decode_jpeg(bytes: &str, max_bytes: usize, max_dimension: u32) -> Option<RgbaImage> {
    if bytes.len() > max_bytes {
        return None;
    }
    let encoded = STANDARD.decode(bytes).ok()?;
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(encoded), image::ImageFormat::Jpeg);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(max_dimension);
    limits.max_image_height = Some(max_dimension);
    limits.max_alloc = Some(u64::from(max_dimension).pow(2) * 8);
    reader.limits(limits);
    Some(reader.decode().ok()?.to_rgba8())
}

fn jpeg(image: RgbaImage) -> Option<String> {
    let mut jpeg = Vec::new();
    let image = image::DynamicImage::ImageRgba8(image).into_rgb8();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 84)
        .encode_image(&image)
        .ok()?;
    Some(STANDARD.encode(&jpeg))
}

fn bundled(provider: Provider) -> Catalog {
    let bytes = match provider {
        Provider::CurseForge => {
            include_bytes!("../home/modpack-catalog/curseforge.json").as_slice()
        }
        Provider::Modrinth => include_bytes!("../home/modpack-catalog/modrinth.json").as_slice(),
    };
    serde_json::from_slice(bytes).expect("bundled modpack catalog")
}

fn cache_path(provider: Provider) -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(local)
            .join("IkaNative/modpack-catalog-v2")
            .join(format!("{}.json", provider.label().to_ascii_lowercase())),
    )
}

pub fn open(provider: Provider) -> Catalog {
    let cached = cache_path(provider).and_then(|path| {
        if path.metadata().ok()?.len() > MAX_CACHE {
            return None;
        }
        let catalog: Catalog = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
        (catalog.projects.len() == PAGE_SIZE * READY_PAGES
            && catalog
                .projects
                .iter()
                .all(|project| project.provider == provider)
            && catalog.first_details.is_some()
            && catalog.page_images(0).len() == PAGE_SIZE)
            .then_some(catalog)
    });
    cached.unwrap_or_else(|| bundled(provider))
}

fn ready_covers(catalog: &mut Catalog, previous: Option<&Catalog>, cap: usize) {
    let mut entries = Vec::new();
    for project in catalog.projects.iter().take(cap) {
        let old_cover = previous.and_then(|old| {
            old.projects
                .iter()
                .find(|p| p.id == project.id && p.icon == project.icon)?;
            old.covers.get(&project.id)
        });
        if let Some(cover) = old_cover {
            catalog.covers.insert(project.id.clone(), cover.clone());
        } else {
            entries.push((project.id.clone(), project.icon.clone()));
        }
    }
    for (id, image) in modpacks::thumbnails(&entries) {
        if let Some(jpeg) = jpeg(image) {
            catalog.covers.insert(id, jpeg);
        }
    }
}

pub fn search(provider: Provider, query: &str, mc: &str) -> Result<Catalog, String> {
    let mut catalog = Catalog {
        projects: modpacks::search(provider, query, mc)?,
        covers: HashMap::new(),
        first_details: None,
    };
    let previous = open(provider);
    ready_covers(&mut catalog, Some(&previous), PAGE_SIZE);
    Ok(catalog)
}

pub fn refresh_next_open(provider: Provider) {
    static CURSEFORGE: AtomicBool = AtomicBool::new(false);
    static MODRINTH: AtomicBool = AtomicBool::new(false);
    let once = match provider {
        Provider::CurseForge => &CURSEFORGE,
        Provider::Modrinth => &MODRINTH,
    };
    if once.swap(true, Ordering::Relaxed) {
        return;
    }
    let Some(path) = cache_path(provider) else {
        return;
    };
    if path
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < Duration::from_secs(6 * 60 * 60))
    {
        return;
    }
    let previous = open(provider);
    let Ok(projects) = modpacks::search(provider, "", "") else {
        return;
    };
    let mut catalog = Catalog {
        projects,
        covers: HashMap::new(),
        first_details: None,
    };
    ready_covers(&mut catalog, Some(&previous), PAGE_SIZE * READY_PAGES);
    // Show complete pages on open. Search still finds packs even if their
    // artwork is unavailable.
    catalog
        .projects
        .retain(|p| catalog.covers.contains_key(&p.id));
    for project in &previous.projects {
        if catalog.projects.len() == PAGE_SIZE * READY_PAGES {
            break;
        }
        if !catalog.covers.contains_key(&project.id)
            && let Some(cover) = previous.covers.get(&project.id)
        {
            catalog.covers.insert(project.id.clone(), cover.clone());
            catalog.projects.push(project.clone());
        }
    }
    if catalog.projects.len() != PAGE_SIZE * READY_PAGES {
        return;
    }
    catalog
        .covers
        .retain(|id, _| catalog.projects.iter().any(|p| &p.id == id));
    let details = modpacks::details(&catalog.projects[0], "");
    let Ok(releases) = details.versions else {
        return;
    };
    if releases.is_empty() {
        return;
    }
    let selected = &catalog.projects[0];
    let hero_url = details
        .gallery
        .iter()
        .find(|url| **url != selected.icon)
        .unwrap_or(&selected.icon);
    let hero = previous
        .first_details
        .as_ref()
        .and_then(|details| details.hero.as_ref())
        .filter(|(url, _)| url == hero_url)
        .cloned()
        .or_else(|| Some((hero_url.clone(), jpeg(modpacks::artwork(hero_url, 720)?)?)));
    catalog.first_details = Some(FirstDetails {
        description: details.description,
        gallery: details.gallery,
        gallery_thumbnails: details.gallery_thumbnails,
        body: details.body,
        releases,
        hero,
    });
    let write = || -> Option<()> {
        let dir = path.parent()?;
        fs::create_dir_all(dir).ok()?;
        let mut file = tempfile::NamedTempFile::new_in(dir).ok()?;
        let bytes = serde_json::to_vec(&catalog).ok()?;
        if bytes.len() as u64 > MAX_CACHE {
            return None;
        }
        file.write_all(&bytes).ok()?;
        file.persist(&path).ok()?;
        Some(())
    };
    let _ = write();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalogs_open_with_real_art_and_installable_version_ids_offline() {
        for provider in [Provider::CurseForge, Provider::Modrinth] {
            let catalog = bundled(provider);
            assert!(catalog.projects.iter().all(|p| p.provider == provider));
            assert_eq!(catalog.projects.len(), PAGE_SIZE * READY_PAGES);
            for page in 0..READY_PAGES {
                let images = catalog.page_images(page);
                assert_eq!(images.len(), PAGE_SIZE, "{provider:?}: missing images");
                assert!(
                    images
                        .iter()
                        .all(|(_, art)| art.width() >= 32 && art.height() >= 32)
                );
            }
            assert!(
                !catalog
                    .first_details()
                    .unwrap()
                    .versions
                    .unwrap()
                    .is_empty()
            );
            assert!(catalog.first_hero().is_some());
        }
    }
}
