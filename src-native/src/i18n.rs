use objc2_foundation::NSLocale;
use std::sync::{atomic::{AtomicBool, Ordering}, LazyLock};

static JAPANESE: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(system_is_japanese()));

fn locale_is_japanese(locale: &str) -> bool {
    locale.split(['-', '_']).next().is_some_and(|language| language.eq_ignore_ascii_case("ja"))
}

fn resolve_language(language: &str, preferred: Option<&str>) -> bool {
    match language {
        "ja" => true,
        "en" => false,
        _ => preferred.is_some_and(locale_is_japanese),
    }
}

fn system_is_japanese() -> bool {
    let preferred = NSLocale::preferredLanguages().iter().next().map(|language| language.to_string());
    resolve_language("system", preferred.as_deref())
}

pub fn set_language(language: &str) {
    let selected = match language { "ja" => true, "en" => false, _ => system_is_japanese() };
    JAPANESE.store(selected, Ordering::Relaxed);
}

pub fn is_japanese() -> bool { JAPANESE.load(Ordering::Relaxed) }

pub fn text(ja: &'static str, en: &'static str) -> &'static str {
    if is_japanese() { ja } else { en }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_uses_the_primary_language_and_falls_back_to_english() {
        for locale in ["ja", "ja-JP", "ja-Latn-JP", "JA_jp"] { assert!(resolve_language("system", Some(locale))); }
        for locale in ["en-JP", "fr-FR", "zh-Hant", "javanese", ""] { assert!(!resolve_language("system", Some(locale))); }
        assert!(!resolve_language("system", None));
    }

    #[test]
    fn an_explicit_choice_overrides_the_system_locale() {
        assert!(resolve_language("ja", Some("en-US")));
        assert!(!resolve_language("en", Some("ja-JP")));
        assert!(resolve_language("system", Some("ja-JP")));
        assert!(!resolve_language("system", Some("en-US")));
    }
}
