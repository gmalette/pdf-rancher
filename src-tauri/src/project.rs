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
            .map(|source_file| Ok((source_file.id.clone(), Document::load(&source_file.path)?)))
            .collect::<Result<Vec<_>>>()?;

        // Define a starting `max_id` (will be used as start index for object_ids).
        let mut max_id = 1;
        let mut pagenum = 1;
        // Collect all Documents Objects grouped by a map
        let mut documents_pages = BTreeMap::new();
        let mut documents_objects = BTreeMap::new();
        let mut document = Document::with_version("1.5");

        let mut source_pages: Vec<Vec<(ObjectId, Object)>> = Vec::new();

        for (_, mut doc) in documents {
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
            } = selector;
            let (object_id, object) = &source_pages[*source_file_id][*page_index];
            if let Ok(dictionary) = object.as_dict() {
                let mut dictionary = dictionary.clone();
                dictionary.set("Parent", pages_object.0);

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
pub struct Selector {
    source_file_index: usize,
    page_index: usize,
}

impl Selector {
    fn new(source_file_index: usize, page_index: usize) -> Self {
        Self {
            source_file_index,
            page_index,
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
    pages: Vec<Page>,
}

impl SourceFile {
    pub fn open(path: &PathBuf) -> Result<Self> {
        // random string
        let id = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(7)
            .map(char::from)
            .collect();
        let path_str = path.to_string_lossy().to_string();
        let pages = load_pdf_pages(path)?;
        Ok(Self {
            id,
            path: path_str,
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

fn load_pdf_pages(path: &PathBuf) -> Result<Vec<Page>> {
    let pdfium = pdfium()?;

    let mut file = File::open(path)?;
    let mut str = Vec::new();
    file.read_to_end(&mut str)?;
    let document = pdfium.load_pdf_from_byte_slice(str.as_bytes(), None)?;

    let render_config = PdfRenderConfig::new()
        .set_target_width(500)
        .set_maximum_height(500);

    let mut previews = Vec::new();

    for page in document.pages().iter() {
        let mut bytes = Cursor::new(Vec::new());

        let img = page
            .render_with_config(&render_config)?
            .as_image()
            .into_rgb8();

        img.write_to(&mut bytes, image::ImageFormat::Jpeg)?;

        previews.push(Page::new(bytes.into_inner(), img.dimensions()));
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
        let source_file = SourceFile::open(&path).unwrap();
        assert_eq!(path.to_string_lossy(), source_file.path);
        assert_eq!(3, source_file.pages.len());
        assert_eq!(386, source_file.pages[0].width());
        assert_eq!(500, source_file.pages[0].height());
    }

    #[test]
    fn test_open_legal() {
        let path = PathBuf::from("test/legal.pdf");
        let source_file = SourceFile::open(&path).unwrap();
        assert_eq!(path.to_string_lossy(), source_file.path);
        assert_eq!(3, source_file.pages.len());
        assert_eq!(304, source_file.pages[0].width());
        assert_eq!(500, source_file.pages[0].height());
    }

    #[test]
    fn test_open_paysage() {
        let path = PathBuf::from("test/paysage.pdf");
        let source_file = SourceFile::open(&path).unwrap();
        assert_eq!(path.to_string_lossy(), source_file.path);
        assert_eq!(3, source_file.pages.len());

        // Paysage pages are rotated 90°
        assert_eq!(500, source_file.pages[0].width());
        assert_eq!(386, source_file.pages[0].height());
    }

    #[test]
    fn test_open_returns_errors() {
        let path = PathBuf::from("test/potato.pdf");
        let source_file = SourceFile::open(&path);
        assert!(source_file.is_err());
        assert_eq!("No such file or directory (os error 2)", source_file.unwrap_err().to_string());
    }

    #[test]
    fn test_merge_documents() {
        let project = Project {
            source_files: vec![
                SourceFile::open(&PathBuf::from("test/basic.pdf")).unwrap(),
                SourceFile::open(&PathBuf::from("test/legal.pdf")).unwrap(),
                SourceFile::open(&PathBuf::from("test/paysage.pdf")).unwrap(),
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
            .filter(|(id, object)| {
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
    fn export_returns_errors() {
        let project = Project {
            source_files: vec![
                SourceFile::open(&PathBuf::from("test/")).unwrap(),
            ],
        };
        let selectors = vec![
            Selector::new(0, 0),
        ];

        let document = project.export(&selectors).unwrap();

        assert_eq!(3, document.page_iter().count());

        let count_streams = document
            .objects
            .iter()
            .filter(|(id, object)| {
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
}
