use crate::error::FavaiError;

// ^[a-z0-9][a-z0-9_-]{0,63}$ — no dots, no slashes, path-safe
pub fn slug(name: &str) -> Result<(), FavaiError> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name.chars().next().map(|c| c.is_ascii_alphanumeric()).unwrap_or(false)
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(FavaiError::InvalidSlug(name.to_string()))
    }
}

// Only https:// and git@…: (ssh) are permitted.
pub fn url_scheme(url: &str) -> Result<(), FavaiError> {
    let allowed = url.starts_with("https://") || url.starts_with("git@");
    if allowed {
        Ok(())
    } else {
        Err(FavaiError::InvalidUrlScheme(url.to_string()))
    }
}
