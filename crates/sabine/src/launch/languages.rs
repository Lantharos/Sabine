/// The languages pages hear the person reads, from `navigator.languages`
/// and in requests, as Chromium lists them for a system language: each of
/// the system's languages, followed by the language without its region, and
/// English last.
pub(crate) fn accept_languages(system: impl IntoIterator<Item = String>) -> String {
    let mut languages = Vec::new();
    for tag in system {
        let base = tag.split('-').next().unwrap_or_default().to_string();
        if !is_language(&base) {
            continue;
        }
        for language in [tag, base] {
            if !languages.contains(&language) {
                languages.push(language);
            }
        }
    }
    for english in ["en-US", "en"] {
        if !languages.iter().any(|language| language == english) {
            languages.push(english.to_string());
        }
    }
    languages.join(",")
}

fn is_language(subtag: &str) -> bool {
    (2..=3).contains(&subtag.len()) && subtag.bytes().all(|byte| byte.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::accept_languages;

    #[test]
    fn lists_regions_then_languages_then_english() {
        let system = ["de-DE", "fr-CH", "C"].map(String::from);
        assert_eq!(accept_languages(system), "de-DE,de,fr-CH,fr,en-US,en");
    }
}
