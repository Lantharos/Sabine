use serde::Deserialize;

const MAX_LANGUAGES: usize = 16;

/// `[spellcheck]` as written in `Sabine.toml`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellcheckSection {
    pub enabled: Option<bool>,
    pub languages: Option<Vec<String>>,
}

impl SpellcheckSection {
    pub fn validate(&self) -> Result<(), String> {
        self.languages
            .as_deref()
            .map_or(Ok(()), validate_spellcheck_languages)
    }
}

/// Checks languages are tags such as `en-US` or `de`, at most sixteen of them.
pub fn validate_spellcheck_languages(languages: &[String]) -> Result<(), String> {
    if languages.len() > MAX_LANGUAGES {
        return Err(format!(
            "spellcheck takes at most {MAX_LANGUAGES} languages"
        ));
    }
    match languages.iter().find(|language| !is_language_tag(language)) {
        Some(language) => Err(format!(
            "spellcheck language `{language}` isn't a language tag such as `en-US`"
        )),
        None => Ok(()),
    }
}

fn is_language_tag(value: &str) -> bool {
    let mut parts = value.split('-');
    parts.next().is_some_and(|language| {
        (2..=3).contains(&language.len()) && language.bytes().all(|byte| byte.is_ascii_alphabetic())
    }) && parts.all(|part| {
        (2..=8).contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_alphanumeric())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_language_tags() {
        let tags = |tags: &[&str]| {
            validate_spellcheck_languages(&tags.iter().map(ToString::to_string).collect::<Vec<_>>())
        };
        assert!(tags(&["en-US", "de", "pt-BR", "sr-Latn"]).is_ok());
        assert!(tags(&["english"]).is_err());
        assert!(tags(&["en_US"]).is_err());
        assert!(tags(&["../en"]).is_err());
    }
}
