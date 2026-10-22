use std::path::Path;

use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat, dictionary};

use super::document::display_name;

const INHERITED: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];
const MAX_TREE_DEPTH: usize = 64;

/// Pages counted from 1 across every file of a print job, both ends kept.
pub(super) type PageRanges = [(u32, u32)];

/// Joins PDF files into one document named `title`, keeping only the pages
/// in `ranges` when there are any.
pub(super) fn combine(
    files: &[&Path],
    ranges: Option<&PageRanges>,
    title: &str,
) -> Result<Vec<u8>, String> {
    let mut combined = Document::with_version("1.7");
    let mut kept = Vec::new();
    let mut number = 0;
    for file in files {
        let mut document =
            Document::load(file).map_err(|_| format!("{} couldn't be read", display_name(file)))?;
        document.renumber_objects_with(combined.max_id + 1);
        for page in document.page_iter() {
            number += 1;
            if ranges.is_none_or(|ranges| {
                ranges
                    .iter()
                    .any(|&(first, last)| (first..=last).contains(&number))
            }) {
                kept.push((page, standalone_page(&document, page)));
            }
        }
        combined.max_id = document.max_id;
        combined.objects.extend(
            document
                .objects
                .into_iter()
                .filter(|(_, object)| !is_page_tree(object)),
        );
    }
    if kept.is_empty() {
        return Err("The chosen pages aren't in the document".into());
    }
    let tree = combined.new_object_id();
    let kids = kept
        .iter()
        .map(|(id, _)| Object::from(*id))
        .collect::<Vec<_>>();
    let count = kids.len() as i64;
    for (id, mut page) in kept {
        page.set("Parent", tree);
        combined.objects.insert(id, Object::Dictionary(page));
    }
    combined.objects.insert(
        tree,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    let catalog = combined.add_object(dictionary! { "Type" => "Catalog", "Pages" => tree });
    let info = combined.add_object(dictionary! { "Title" => text_string(title) });
    combined.trailer.set("Root", catalog);
    combined.trailer.set("Info", info);
    let mut bytes = Vec::new();
    combined
        .save_to(&mut bytes)
        .map_err(|error| format!("The document couldn't be put together: {error}"))?;
    Ok(bytes)
}

/// A copy of a page carrying the attributes it inherits from its page tree,
/// so it stands on its own under a new tree.
fn standalone_page(document: &Document, id: ObjectId) -> Dictionary {
    let mut page = document.get_dictionary(id).cloned().unwrap_or_default();
    let ancestors = std::iter::successors(parent(document, &page), |node| parent(document, node));
    for node in ancestors.take(MAX_TREE_DEPTH) {
        for key in INHERITED {
            if !page.has(key)
                && let Ok(value) = node.get(key)
            {
                page.set(key, value.clone());
            }
        }
    }
    page
}

fn parent<'a>(document: &'a Document, node: &Dictionary) -> Option<&'a Dictionary> {
    let id = node.get(b"Parent").and_then(Object::as_reference).ok()?;
    document.get_dictionary(id).ok()
}

fn text_string(text: &str) -> Object {
    let bytes = [0xfe, 0xff]
        .into_iter()
        .chain(text.encode_utf16().flat_map(u16::to_be_bytes))
        .collect();
    Object::String(bytes, StringFormat::Hexadecimal)
}

fn is_page_tree(object: &Object) -> bool {
    matches!(
        object.type_name(),
        Ok(b"Catalog" | b"Pages" | b"Page" | b"Outlines" | b"Outline")
    )
}
