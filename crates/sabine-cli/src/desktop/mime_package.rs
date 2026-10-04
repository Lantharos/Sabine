use super::types::Associations;

/// A shared-mime-info package teaching Linux desktops the app's file
/// extensions, or `None` when no document type lists extensions.
pub(crate) fn mime_package(associations: &Associations) -> Option<String> {
    let types = associations
        .documents()
        .filter(|document| !document.extensions.is_empty())
        .map(|document| {
            let globs = document
                .extensions
                .iter()
                .map(|extension| format!("    <glob pattern=\"*.{extension}\"/>\n"))
                .collect::<String>();
            format!(
                "  <mime-type type=\"{}\">\n{globs}  </mime-type>\n",
                document.mime_type
            )
        })
        .collect::<String>();
    (!types.is_empty()).then(|| {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<mime-info xmlns=\"http://www.freedesktop.org/standards/shared-mime-info\">\n{types}</mime-info>\n"
        )
    })
}
