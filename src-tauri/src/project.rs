use anyhow::{anyhow, Result};
use image::EncodableLayout;
use lopdf::{Bookmark, Document, Object, ObjectId};
use pdfium_render::prelude::*;
use rand::distributions::Alphanumeric;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env::consts::{ARCH, OS};
use std::ffi::OsString;
use std::fmt::Debug;
use std::fs::File;
use std::io::{Cursor, Read};
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

    pub fn add_source_files(&mut self, new_files: Vec<SourceFile>) {
        self.source_files.extend(new_files);
    }

    pub fn export(&self, selectors: &Vec<Selector>) -> Result<Document> {
        // Load documents
        let documents = self
            .source_files
            .iter()
            .map(|source_file| source_file.document.clone())
            .collect::<Vec<_>>();

        // Define a starting `max_id` (will be used as start index for object_ids).
        let mut max_id = 1;
        let mut pagenum = 1;
        // Collect all Documents Objects grouped by a map
        let mut documents_pages = BTreeMap::new();
        let mut documents_objects = BTreeMap::new();
        let mut document = Document::with_version("1.5");

        let mut source_pages: Vec<Vec<(ObjectId, Object)>> = Vec::new();

        for mut doc in documents.into_iter() {
            let mut first = false;
            let mut source_page = Vec::new();

            doc.renumber_objects_with(max_id);

            max_id = doc.max_id + 1;

            documents_pages.extend(
                doc.get_pages()
                    .into_iter()
                    .map(|(_, object_id)| {
                        if !first {
                            let bookmark = Bookmark::new(
                                String::from(format!("Page_{}", pagenum)),
                                [0.0, 0.0, 1.0],
                                0,
                                object_id,
                            );
                            document.add_bookmark(bookmark, None);
                            first = true;
                            pagenum += 1;
                        }

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
            match object.type_name().unwrap_or("") {
                "Catalog" => {
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
                "Pages" => {
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
                "Page" => {}     // Ignored, processed later and separately
                "Outlines" => {} // Ignored, not supported yet
                "Outline" => {}  // Ignored, not supported yet
                _ => {
                    document.objects.insert(*object_id, object.clone());
                }
            }
        }

        // If no "Pages" object found, abort.
        let pages_object = pages_object.expect("Invalid PDF: Pages root not found.");

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

        let catalog_object = catalog_object.expect("Invalid PDF: Catalog root not found.");
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

    fn width(&self) -> u32 {
        self.dimensions.0
    }

    fn height(&self) -> u32 {
        self.dimensions.1
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceFile {
    id: String,
    path: String,
    #[serde(skip_serializing)]
    document: Document,
    pages: Vec<Page>,
}

impl SourceFile {
    pub fn open(path: &PathBuf, sender: Option<mpsc::Sender<(usize, usize)>>) -> Result<Self> {
        let mut bytes = Vec::new();
        File::open(path)?.read_to_end(&mut bytes)?;

        let offset = bytes[..1024].windows(5).position(|w| w == b"%PDF-").ok_or_else(|| {
            anyhow!("Failed to find PDF header in file {}", path.to_string_lossy())
        })?;

        let reader = Cursor::new(&bytes[offset..]);

        let document = Document::load_from(reader)?;
        // random string
        let id = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(7)
            .map(char::from)
            .collect();
        let path_str = path.to_string_lossy().to_string();
        let pages = load_pdf_pages(path, sender)?;

        Ok(Self {
            id,
            path: path_str,
            document,
            pages,
        })
    }

    pub fn pages(&self) -> impl Iterator<Item = &Page> {
        self.pages.iter()
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

        if let Ok(lib) = Pdfium::bind_to_library(name) {
            return Ok(Pdfium::new(lib));
        }
    }

    Err(anyhow!("Failed to load Pdfium library"))
}

fn load_pdf_pages(path: &PathBuf, sender: Option<mpsc::Sender<(usize, usize)>>) -> Result<Vec<Page>> {
    let pdfium = pdfium()?;

    let mut file = File::open(path)?;
    let mut str = Vec::new();
    file.read_to_end(&mut str)?;
    let document = pdfium.load_pdf_from_byte_slice(str.as_bytes(), None)?;

    let render_config = PdfRenderConfig::new()
        .set_target_width(800)
        .set_maximum_height(800);

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
        assert_eq!(path.to_string_lossy(), source_file.path);
        assert_eq!(3, source_file.pages.len());
        assert_eq!(618, source_file.pages[0].width());
        assert_eq!(800, source_file.pages[0].height());
    }

    #[test]
    fn test_open_legal() {
        let path = PathBuf::from("test/legal.pdf");
        let source_file = SourceFile::open(&path, None).unwrap();
        assert_eq!(path.to_string_lossy(), source_file.path);
        assert_eq!(3, source_file.pages.len());
        assert_eq!(486, source_file.pages[0].width());
        assert_eq!(800, source_file.pages[0].height());
    }

    #[test]
    fn test_open_paysage() {
        let path = PathBuf::from("test/paysage.pdf");
        let source_file = SourceFile::open(&path, None).unwrap();
        assert_eq!(path.to_string_lossy(), source_file.path);
        assert_eq!(3, source_file.pages.len());

        // Paysage pages are rotated 90°
        assert_eq!(800, source_file.pages[0].width());
        assert_eq!(618, source_file.pages[0].height());
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

    // TODO
    fn export_returns_errors() {
        let project = Project {
            source_files: vec![SourceFile::open(&PathBuf::from("test/"), None).unwrap()],
        };
        let selectors = vec![Selector::new(0, 0)];

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
