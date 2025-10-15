use anyhow::{anyhow, Result};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use pdfium_render::prelude::*;
use rand::distr::Alphanumeric;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env::consts::{ARCH, OS};
use std::ffi::OsString;
use std::fmt::Debug;
use std::fs::File;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::mpsc;

#[derive(Debug, Clone, Serialize)]
pub struct Project {
    source_files: Vec<SourceFile>,
}

impl Project {
    pub fn new() -> Self {
        Self {
            source_files: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.source_files.clear()
    }

    pub fn add_source_files(&mut self, new_files: Vec<SourceFile>) {
        self.source_files.extend(new_files);
    }

    pub fn preview(&self, selector: Selector) -> Result<Page> {
        let source_file = self
            .source_files
            .get(selector.source_file_index)
            .ok_or_else(|| anyhow!("Invalid source_file_index: {}", selector.source_file_index))?;

        let pdfium = pdfium()?;
        let bytes = source_file.to_bytes()?;
        let document = pdfium.load_pdf_from_byte_slice(&bytes, None)?;

        let render_config = PdfRenderConfig::new()
            .set_target_width(800)
            .set_maximum_height(800);

        let page = document
            .pages()
            .iter()
            .nth(selector.page_index)
            .ok_or_else(|| anyhow!("Invalid page_index: {}", selector.page_index))?;
        let img = page
            .render_with_config(&render_config)?
            .as_image()
            .into_rgb8();
        let mut bytes = Cursor::new(Vec::new());

        img.write_to(&mut bytes, image::ImageFormat::Jpeg)?;

        Ok(Page::new(bytes.into_inner(), img.dimensions()))
    }

    pub fn export(&self, selectors: &Vec<Selector>) -> Result<Document> {
        // Basic validations to avoid panics
        if self.source_files.is_empty() {
            return Err(anyhow!("No source files to export"));
        }
        for (i, sel) in selectors.iter().enumerate() {
            let Some(sf) = self.source_files.get(sel.source_file_index) else {
                return Err(anyhow!(
                    "Selector {} has invalid source_file_index: {}",
                    i,
                    sel.source_file_index
                ));
            };
            if sel.page_index >= sf.pages.len() {
                return Err(anyhow!(
                    "Selector {} has invalid page_index: {} (source {} has {} pages)",
                    i,
                    sel.page_index,
                    sel.source_file_index,
                    sf.pages.len()
                ));
            }
        }

        // Load documents
        let documents = self
            .source_files
            .iter()
            .map(|source_file| source_file.document.clone())
            .collect::<Vec<_>>();

        // Define a starting `max_id` (will be used as start index for object_ids).
        let mut max_id = 1;
        // Collect all Documents Objects grouped by a map
        let mut documents_pages = BTreeMap::new();
        let mut documents_objects = BTreeMap::new();
        let mut document = Document::with_version("1.5");

        let mut source_pages: Vec<Vec<(ObjectId, Object)>> = Vec::new();

        for mut doc in documents.into_iter() {
            let mut source_page = Vec::new();

            doc.renumber_objects_with(max_id);

            max_id = doc.max_id + 1;

            documents_pages.extend(
                doc.get_pages()
                    .into_iter()
                    .map(|(_, object_id)| {
                        let page = doc.get_object(object_id)?;

                        source_page.push((object_id, page.to_owned()));

                        Ok((object_id, page.to_owned()))
                    })
                    .collect::<Result<BTreeMap<ObjectId, Object>>>()?,
            );

            source_pages.push(source_page);
            documents_objects.extend(doc.objects);
        }

        // "Catalog" and "Pages" are mandatory.
        let mut catalog_object: Option<(ObjectId, Object)> = None;
        let mut pages_object: Option<(ObjectId, Object)> = None;

        // Process all objects except "Page" type
        for (object_id, object) in documents_objects.iter() {
            // We have to ignore "Page" (as are processed later), "Outlines" and "Outline" objects.
            // All other objects should be collected and inserted into the main Document.
            match object.type_name().unwrap_or(b"") {
                b"Catalog" => {
                    // Collect a first "Catalog" object and use it for the future "Pages".
                    catalog_object = Some((
                        if let Some((id, _)) = catalog_object {
                            id
                        } else {
                            *object_id
                        },
                        object.clone(),
                    ));
                }
                b"Pages" => {
                    // Collect and update a first "Pages" object and use it for the future "Catalog"
                    // We have also to merge all dictionaries of the old and the new "Pages" object
                    if let Ok(dictionary) = object.as_dict() {
                        let mut dictionary = dictionary.clone();
                        if let Some((_, ref object)) = pages_object {
                            if let Ok(old_dictionary) = object.as_dict() {
                                dictionary.extend(old_dictionary);
                            }
                        }

                        pages_object = Some((
                            if let Some((id, _)) = pages_object {
                                id
                            } else {
                                *object_id
                            },
                            Object::Dictionary(dictionary),
                        ));
                    }
                }
                b"Page" => {}     // Ignored, processed later and separately
                b"Outlines" => {} // Ignored, not supported yet
                b"Outline" => {}  // Ignored, not supported yet
                _ => {
                    document.objects.insert(*object_id, object.clone());
                }
            }
        }

        // If no "Pages" object found, abort.
        let Some(pages_object) = pages_object else {
            return Err(anyhow!("Invalid PDF: Pages root not found."));
        };

        // Iterate over all "Page" objects and collect into the parent "Pages" created before
        let mut selected_pages = Vec::new();
        for selector in selectors.iter() {
            let Selector {
                source_file_index: source_file_id,
                page_index,
                rotation,
            } = selector;
            let (object_id, object) = &source_pages[*source_file_id][*page_index];
            if let Ok(dictionary) = object.as_dict() {
                let mut dictionary = dictionary.clone();
                dictionary.set("Parent", pages_object.0);

                rotation.as_rotation().map(|r| dictionary.set("Rotate", r));

                selected_pages.push(*object_id);

                document
                    .objects
                    .insert(*object_id, Object::Dictionary(dictionary));
            }
        }

        let Some(catalog_object) = catalog_object else {
            return Err(anyhow!("Invalid PDF: Catalog root not found."));
        };
        let pages_object = pages_object;

        // Build a new "Pages" with updated fields
        if let Ok(dictionary) = pages_object.1.as_dict() {
            let mut dictionary = dictionary.clone();

            // Set new pages count
            dictionary.set("Count", selectors.len() as u32);

            // Set new "Kids" list (collected from documents pages) for "Pages"
            dictionary.set(
                "Kids",
                selected_pages
                    .into_iter()
                    .map(|object_id| Object::Reference(object_id))
                    .collect::<Vec<_>>(),
            );

            document
                .objects
                .insert(pages_object.0, Object::Dictionary(dictionary));
        }

        // Build a new "Catalog" with updated fields
        if let Ok(dictionary) = catalog_object.1.as_dict() {
            let mut dictionary = dictionary.clone();
            dictionary.set("Pages", pages_object.0);
            dictionary.remove(b"Outlines"); // Outlines not supported in merged PDFs

            document
                .objects
                .insert(catalog_object.0, Object::Dictionary(dictionary));
        }

        document.trailer.set("Root", catalog_object.0);

        // Update the max internal ID as wasn't updated before due to direct objects insertion
        document.max_id = document.objects.len() as u32;

        // Reorder all new Document objects
        document.renumber_objects();

        // Set any Bookmarks to the First child if they are not set to a page
        document.adjust_zero_pages();

        // Set all bookmarks to the PDF Object tree then set the Outlines to the Bookmark content map.
        if let Some(n) = document.build_outline() {
            if let Ok(x) = document.get_object_mut(catalog_object.0) {
                if let Object::Dictionary(ref mut dict) = x {
                    dict.set("Outlines", Object::Reference(n));
                }
            }
        }

        document.prune_objects();
        document.compress();

        // Save the merged PDF.
        // Store file in current working directory.
        // Note: Line is excluded when running doc tests
        // if false {
        // document.save("merged.pdf").unwrap();
        // }
        Ok(document)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
enum Rotation {
    // serialize as just "0"
    #[serde(rename = "0")]
    R0,
    #[serde(rename = "90")]
    R90,
    #[serde(rename = "180")]
    R180,
    #[serde(rename = "270")]
    R270,
}

impl Rotation {
    fn as_rotation(&self) -> Option<u32> {
        match self {
            Rotation::R0 => None,
            Rotation::R90 => Some(90),
            Rotation::R180 => Some(180),
            Rotation::R270 => Some(270),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Selector {
    source_file_index: usize,
    page_index: usize,
    rotation: Rotation,
}

impl Selector {
    #[cfg(test)]
    fn new(source_file_index: usize, page_index: usize) -> Self {
        Self {
            source_file_index,
            page_index,
            rotation: Rotation::R0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Page {
    #[serde(with = "base64")]
    preview_jpg: Vec<u8>,
    dimensions: (u32, u32),
}

impl Page {
    fn new(preview_jpg: Vec<u8>, dimensions: (u32, u32)) -> Self {
        Self {
            preview_jpg,
            dimensions,
        }
    }

    #[cfg(test)]
    fn width(&self) -> u32 {
        self.dimensions.0
    }

    #[cfg(test)]
    fn height(&self) -> u32 {
        self.dimensions.1
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
enum Source {
    PDF(PathBuf),
    Image(PathBuf),
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceFile {
    id: String,
    source: Source,
    #[serde(skip_serializing)]
    document: Document,
    pages: Vec<Page>,
}

impl SourceFile {
    pub fn open(path: &PathBuf, sender: Option<mpsc::Sender<(usize, usize)>>) -> Result<Self> {
        // Determine file extension
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase());

        let id: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(7)
            .map(char::from)
            .collect();

        match ext.as_deref() {
            Some("pdf") => {
                // Current PDF implementation
                let reader = File::open(path)?;
                let document = Document::load_from(reader)?;
                // random string
                let pages = load_pdf_pages(&document, sender)?;

                Ok(Self {
                    id,
                    source: Source::PDF(path.clone()),
                    document,
                    pages,
                })
            }
            // Image branch (extensions supported by the `image` crate)
            Some(ext) if image::ImageFormat::from_extension(ext).is_some() => {
                // Load image
                let dyn_img = image::open(path)?;
                let rgba = dyn_img.to_rgba8();
                let (img_w, img_h) = rgba.dimensions();

                // Page sizes in points for A4
                const A4_PORTRAIT: (f64, f64) = (595.0, 842.0);
                const A4_LANDSCAPE: (f64, f64) = (842.0, 595.0);
                const MARGIN: f64 = 36.0; // 0.5 inch

                // Helper to compute best scale for given page
                let fit_scale = |pw: f64, ph: f64| -> f64 {
                    let cw = (pw - 2.0 * MARGIN).max(1.0);
                    let ch = (ph - 2.0 * MARGIN).max(1.0);
                    let sx = cw / (img_w as f64);
                    let sy = ch / (img_h as f64);
                    sx.min(sy).min(1.0)
                };

                // Decide orientation
                let sp = fit_scale(A4_PORTRAIT.0, A4_PORTRAIT.1);
                let sl = fit_scale(A4_LANDSCAPE.0, A4_LANDSCAPE.1);

                let (page_w, page_h, scale) = if (img_w as f64) <= (A4_PORTRAIT.0 - 2.0 * MARGIN)
                    && (img_h as f64) <= (A4_PORTRAIT.1 - 2.0 * MARGIN)
                {
                    (A4_PORTRAIT.0, A4_PORTRAIT.1, sp)
                } else if img_w > img_h && sl >= sp {
                    (A4_LANDSCAPE.0, A4_LANDSCAPE.1, sl)
                } else if sp >= sl {
                    (A4_PORTRAIT.0, A4_PORTRAIT.1, sp)
                } else {
                    (A4_LANDSCAPE.0, A4_LANDSCAPE.1, sl)
                };

                // Calculate target dimensions for the image in the PDF
                let target_w = ((img_w as f64) * scale).round() as u32;
                let target_h = ((img_h as f64) * scale).round() as u32;

                // Resize image if it's larger than the target dimensions
                // This significantly reduces memory usage and file size for large images
                let rgba = if target_w < img_w || target_h < img_h {
                    image::DynamicImage::ImageRgba8(rgba)
                        .resize(target_w, target_h, image::imageops::FilterType::Lanczos3)
                        .to_rgba8()
                } else {
                    rgba
                };

                let (img_w, img_h) = rgba.dimensions();

                // Display size and position (top-left)
                let display_w = img_w as f64;
                let display_h = img_h as f64;
                let pos_x = MARGIN;
                // PDF origin is bottom-left; to place at top-left, translate so top aligns with page_h - MARGIN
                let pos_y = page_h - MARGIN - display_h;

                // Prepare image data and optional SMask for alpha
                let mut rgb = Vec::with_capacity((img_w * img_h * 3) as usize);
                let mut alpha = Vec::with_capacity((img_w * img_h) as usize);
                for px in rgba.pixels() {
                    rgb.push(px.0[0]);
                    rgb.push(px.0[1]);
                    rgb.push(px.0[2]);
                    alpha.push(px.0[3]);
                }
                let alpha = if rgba.pixels().any(|p| p.0[3] < 255) {
                    Some(alpha)
                } else {
                    None
                };

                let mut doc = Document::with_version("1.5");

                // Create main image XObject
                let mut img_dict = Dictionary::new();
                img_dict.set("Type", "XObject");
                img_dict.set("Subtype", "Image");
                img_dict.set("Width", img_w as i64);
                img_dict.set("Height", img_h as i64);
                img_dict.set("ColorSpace", "DeviceRGB");
                img_dict.set("BitsPerComponent", 8);

                // Optional Transparency SMask
                if let Some(alpha_bytes) = alpha {
                    // Create SMask image (grayscale, 8 bpc)
                    let mut smask_dict = Dictionary::new();
                    smask_dict.set("Type", "XObject");
                    smask_dict.set("Subtype", "Image");
                    smask_dict.set("Width", img_w as i64);
                    smask_dict.set("Height", img_h as i64);
                    smask_dict.set("ColorSpace", "DeviceGray");
                    smask_dict.set("BitsPerComponent", 8);
                    let smask_stream = Stream::new(smask_dict, alpha_bytes);
                    let smask_id = doc.add_object(Object::Stream(smask_stream));
                    img_dict.set("SMask", Object::Reference(smask_id));
                };

                let img_stream = Stream::new(img_dict, rgb);
                let img_id = doc.add_object(Object::Stream(img_stream));

                // Resources dictionary with XObject name
                let mut xobjects = Dictionary::new();
                xobjects.set("Im0", Object::Reference(img_id));
                let mut resources = Dictionary::new();
                resources.set("XObject", Object::Dictionary(xobjects));

                // Content stream to draw the image at top-left with scaling
                let content = format!(
                    "q\n{} 0 0 {} {} {} cm\n/Im0 Do\nQ\n",
                    display_w, display_h, pos_x, pos_y
                );
                let content_stream = Stream::new(Dictionary::new(), content.into_bytes());
                let content_id = doc.add_object(Object::Stream(content_stream));

                // Page dictionary
                let mut page = Dictionary::new();
                page.set("Type", "Page");
                page.set(
                    "MediaBox",
                    vec![
                        Object::Integer(0),
                        Object::Integer(0),
                        Object::Real(page_w as f32),
                        Object::Real(page_h as f32),
                    ],
                );
                page.set("Resources", Object::Dictionary(resources));
                page.set("Contents", Object::Reference(content_id));

                let page_id = doc.add_object(Object::Dictionary(page));

                // Pages tree
                let mut pages = Dictionary::new();
                pages.set("Type", "Pages");
                pages.set("Kids", vec![Object::Reference(page_id)]);
                pages.set("Count", 1);
                let pages_id = doc.add_object(Object::Dictionary(pages));

                // Set parent on page
                if let Some(Object::Dictionary(ref mut d)) = doc.objects.get_mut(&page_id) {
                    d.set("Parent", Object::Reference(pages_id));
                }

                // Catalog
                let mut catalog = Dictionary::new();
                catalog.set("Type", "Catalog");
                catalog.set("Pages", Object::Reference(pages_id));
                let catalog_id = doc.add_object(Object::Dictionary(catalog));
                doc.trailer.set("Root", catalog_id);

                let pages = load_pdf_pages(&doc, sender)?;

                Ok(Self {
                    id,
                    source: Source::Image(path.clone()),
                    document: doc,
                    pages,
                })
            }
            Some(other) => Err(anyhow!("Unsupported file extension: {}", other)),
            None => Err(anyhow!("File has no extension: {}", path.to_string_lossy())),
        }
    }

    fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.document.clone().save_to(&mut bytes)?;
        Ok(bytes)
    }
}

fn pdfium() -> Result<Pdfium> {
    for path in ["./", "./frameworks/"].iter() {
        let mut prefix = OsString::new();
        prefix.push(path);
        prefix.push(ARCH);
        prefix.push("-");
        prefix.push(OS);

        let name = Pdfium::pdfium_platform_library_name_at_path(&prefix);

        let val = Pdfium::bind_to_library(name);
        if let Ok(lib) = val {
            return Ok(Pdfium::new(lib));
        }
    }

    Err(anyhow!("Failed to load Pdfium library"))
}

fn load_pdf_pages(
    document: &Document,
    sender: Option<mpsc::Sender<(usize, usize)>>,
) -> Result<Vec<Page>> {
    let pdfium = pdfium()?;

    let mut bytes = Vec::new();
    document.clone().save_to(&mut bytes)?;
    let document = pdfium.load_pdf_from_byte_slice(&bytes, None)?;

    let render_config = PdfRenderConfig::new()
        .set_target_width(300)
        .set_maximum_height(300);

    let mut previews = Vec::new();

    let page_count = document.pages().len() as usize;
    for (index, page) in document.pages().iter().enumerate() {
        let mut bytes = Cursor::new(Vec::new());

        let img = page
            .render_with_config(&render_config)?
            .as_image()
            .into_rgb8();

        img.write_to(&mut bytes, image::ImageFormat::Jpeg)?;

        previews.push(Page::new(bytes.into_inner(), img.dimensions()));

        if let Some(sender) = &sender {
            let _ = sender.send((index + 1, page_count));
        }
    }

    Ok(previews)
}

mod base64 {
    use base64::prelude::*;
    use serde::Serialize;
    use serde::Serializer;

    pub fn serialize<S: Serializer>(v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        let base64 = BASE64_STANDARD.encode(v);
        String::serialize(&base64, s)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_open() {
        let path = PathBuf::from("test/basic.pdf");
        let source_file = SourceFile::open(&path, None).unwrap();
        assert_eq!(Source::PDF(path), source_file.source);
        assert_eq!(3, source_file.pages.len());
        assert_eq!(232, source_file.pages[0].width());
        assert_eq!(300, source_file.pages[0].height());
    }

    #[test]
    fn test_open_offset() {
        let path = PathBuf::from("test/offset.pdf");
        let source_file = SourceFile::open(&path, None).unwrap();
        assert_eq!(Source::PDF(path), source_file.source);
        assert_eq!(3, source_file.pages.len());
        assert_eq!(232, source_file.pages[0].width());
        assert_eq!(300, source_file.pages[0].height());
    }

    #[test]
    fn test_clear() {
        let path = PathBuf::from("test/basic.pdf");
        let source_file = SourceFile::open(&path, None).unwrap();
        let mut project = Project::new();
        project.add_source_files(vec![source_file.clone()]);

        assert_eq!(1, project.source_files.len());

        project.clear();

        assert_eq!(0, project.source_files.len());
    }

    #[test]
    fn test_open_legal() {
        let path = PathBuf::from("test/legal.pdf");
        let source_file = SourceFile::open(&path, None).unwrap();
        assert_eq!(Source::PDF(path), source_file.source);
        assert_eq!(3, source_file.pages.len());
        assert_eq!(182, source_file.pages[0].width());
        assert_eq!(300, source_file.pages[0].height());
    }

    #[test]
    fn test_open_paysage() {
        let path = PathBuf::from("test/paysage.pdf");
        let source_file = SourceFile::open(&path, None).unwrap();
        assert_eq!(Source::PDF(path), source_file.source);
        assert_eq!(3, source_file.pages.len());

        // Paysage pages are rotated 90°
        assert_eq!(300, source_file.pages[0].width());
        assert_eq!(232, source_file.pages[0].height());
    }

    #[test]
    fn test_open_small_image_jpg() {
        let path = PathBuf::from("test/small-image.jpg");
        let source_file = SourceFile::open(&path, None).unwrap();
        assert_eq!(Source::Image(path), source_file.source);
        // A single page is produced for an image
        assert_eq!(1, source_file.pages.len());
        // Preview should have non-zero dimensions
        assert!(source_file.pages[0].width() > 0);
        assert!(source_file.pages[0].height() > 0);
    }

    #[test]
    fn test_open_large_image_jpg() {
        let path = PathBuf::from("test/large-image.jpg");
        let source_file = SourceFile::open(&path, None).unwrap();
        assert_eq!(Source::Image(path), source_file.source);
        // A single page is produced for an image
        assert_eq!(1, source_file.pages.len());
        // Preview should have non-zero dimensions
        assert!(source_file.pages[0].width() > 0);
        assert!(source_file.pages[0].height() > 0);
    }

    #[test]
    fn test_open_returns_errors() {
        let path = PathBuf::from("test/potato.pdf");
        let source_file = SourceFile::open(&path, None);
        assert!(source_file.is_err());
        assert_eq!(
            "No such file or directory (os error 2)",
            source_file.unwrap_err().to_string()
        );
    }

    #[test]
    fn test_open_no_extension_returns_error() {
        let path = PathBuf::from("test/potato");
        let source_file = SourceFile::open(&path, None);
        assert!(source_file.is_err());
        assert_eq!(
            "File has no extension: test/potato",
            source_file.unwrap_err().to_string()
        );
    }

    #[test]
    fn test_merge_documents() {
        let project = Project {
            source_files: vec![
                SourceFile::open(&PathBuf::from("test/basic.pdf"), None).unwrap(),
                SourceFile::open(&PathBuf::from("test/legal.pdf"), None).unwrap(),
                SourceFile::open(&PathBuf::from("test/paysage.pdf"), None).unwrap(),
            ],
        };
        let selectors = vec![
            Selector::new(0, 0),
            Selector::new(1, 1),
            Selector::new(2, 2),
        ];

        let document = project.export(&selectors).unwrap();

        assert_eq!(3, document.page_iter().count());

        let count_streams = document
            .objects
            .iter()
            .filter(|(_id, object)| {
                if let Object::Stream(s) = object {
                    let contents = s.decompressed_content().unwrap();
                    // The streams would contain (1), (2), or (3)
                    contents
                        .windows(3)
                        .find(|w| w == &[40, 49, 41] || w == &[40, 50, 41] || w == &[40, 51, 41])
                        .is_some()
                } else {
                    false
                }
            })
            .count();

        // Make sure hidden objects have been pruned
        assert_eq!(3, count_streams);
    }

    #[test]
    fn test_export_invalid_source_index_errors() {
        let project = Project {
            source_files: vec![SourceFile::open(&PathBuf::from("test/basic.pdf"), None).unwrap()],
        };
        // Select a source file index that does not exist (only 0 exists)
        let selectors = vec![Selector::new(1, 0)];
        let result = project.export(&selectors);
        assert!(result.is_err());
    }

    #[test]
    fn test_export_invalid_page_index_errors() {
        let project = Project {
            source_files: vec![SourceFile::open(&PathBuf::from("test/basic.pdf"), None).unwrap()],
        };
        // Select a page index that does not exist in the source (basic.pdf has only 3 pages)
        let selectors = vec![Selector::new(0, 99)];
        let result = project.export(&selectors);
        assert!(result.is_err());
    }

    #[test]
    fn test_export_with_no_sources_errors() {
        let project = Project {
            source_files: vec![],
        };
        // Any selector will be invalid since there are no sources
        let selectors = vec![Selector::new(0, 0)];
        let result = project.export(&selectors);
        assert!(result.is_err());
    }

    #[test]
    fn test_rotate() {
        let project = Project {
            source_files: vec![SourceFile::open(&PathBuf::from("test/basic.pdf"), None).unwrap()],
        };

        let selectors = vec![
            Selector {
                source_file_index: 0,
                page_index: 0,
                rotation: Rotation::R0,
            },
            Selector {
                source_file_index: 0,
                page_index: 1,
                rotation: Rotation::R270,
            },
            Selector {
                source_file_index: 0,
                page_index: 2,
                rotation: Rotation::R90,
            },
        ];

        let document = project.export(&selectors).unwrap();

        let pages = document.page_iter().collect::<Vec<_>>();

        assert_eq!(3, pages.len());

        assert_eq!(
            None,
            document
                .get_dictionary(pages[0])
                .unwrap()
                .get("Rotate".as_bytes())
                .ok(),
        );
        assert_eq!(
            270,
            document
                .get_dictionary(pages[1])
                .unwrap()
                .get("Rotate".as_bytes())
                .unwrap()
                .as_i64()
                .unwrap()
        );
        assert_eq!(
            90,
            document
                .get_dictionary(pages[2])
                .unwrap()
                .get("Rotate".as_bytes())
                .unwrap()
                .as_i64()
                .unwrap()
        );
    }
}
