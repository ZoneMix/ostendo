use crate::theme::schema::Theme;

include!(concat!(env!("OUT_DIR"), "/theme_list.rs"));

pub fn load_builtin_themes() -> Vec<Theme> {
    BUILTIN_THEMES
        .iter()
        .filter_map(|(slug, yaml)| {
            let mut theme: Theme = serde_yaml::from_str(yaml).ok()?;
            if theme.slug.is_empty() {
                theme.slug = slug.to_string();
            }
            Some(theme)
        })
        .collect()
}
