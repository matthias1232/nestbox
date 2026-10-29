//! Validation for the selections an operator assembles in the builder.
//!
//! Packages and repositories reach a generated Dockerfile and a `docker exec`,
//! so they are validated once here and quoted where they are used. Keeping the
//! rule in one place means the recipe path and the template path cannot disagree
//! about what a legal package name is.

use crate::domain::builder::RepositorySpec;

/// A package name the native package managers accept.
///
/// Deliberately strict: letters, digits and `+._-` only. A version pin
/// (`name=1.2`) and a repository-qualified name (`repo/package`) are expressed by
/// separate fields, not by loosening this one.
pub fn package(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 {
        return Err("A package name is 1 to 128 characters.".to_string());
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "+._-".contains(c))
    {
        return Err(format!("'{value}' is not a package name."));
    }
    Ok(value.to_string())
}

/// A repository name: a DNS label, so it is safe as a file and a keyring name.
pub fn repository_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 63 {
        return Err("A repository name is 1 to 63 characters.".to_string());
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || value.starts_with('-')
    {
        return Err(format!(
            "'{value}' must be lowercase letters, digits, and hyphens."
        ));
    }
    Ok(value.to_string())
}

/// A repository URL: HTTPS, no whitespace and no shell metacharacters.
pub fn repository_url(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 1024 {
        return Err("A repository URL is 1 to 1024 characters.".to_string());
    }
    if !value.starts_with("https://") {
        return Err(format!("'{value}' is not an HTTPS URL."));
    }
    if value.chars().any(|c| {
        c.is_whitespace() || c.is_control() || matches!(c, '\'' | '"' | '`' | '\\' | '$' | '|' | ';')
    }) {
        return Err(format!(
            "'{value}' contains characters that are not allowed in a repository URL."
        ));
    }
    Ok(value.to_string())
}

/// Validate one repository and return its normalised form.
///
/// The signing key is public material and stays with the repository. A private
/// credential is a vault locator (`db://name`), never a value: the database and
/// the build manifest must not be able to leak one.
pub fn repository(spec: &RepositorySpec) -> Result<RepositorySpec, String> {
    if spec.credential_ref.as_deref().is_some_and(|reference| !reference.starts_with("db://")) {
        return Err("A repository credential reference must be a db:// vault locator.".to_string());
    }
    if spec.signing_key.as_deref().is_some_and(|key| key.len() > 64 * 1024) {
        return Err("A repository signing key is at most 64 KiB.".to_string());
    }
    let components = spec
        .components
        .iter()
        .map(|component| component.trim().to_string())
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    if components.iter().any(|component| {
        !component
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+._-".contains(c))
    }) {
        return Err("A repository component is a word like 'main' or 'stable'.".to_string());
    }
    Ok(RepositorySpec {
        name: repository_name(&spec.name)?,
        url: repository_url(&spec.url)?,
        suite: spec
            .suite
            .as_deref()
            .map(str::trim)
            .filter(|suite| !suite.is_empty())
            .map(str::to_string),
        components,
        signing_key: spec.signing_key.clone(),
        credential_ref: spec.credential_ref.clone(),
    })
}

/// Validate package names, preserving the operator's order and dropping repeats.
pub fn packages(values: &[String]) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::with_capacity(values.len());
    for value in values {
        let package = package(value)?;
        if !out.contains(&package) {
            out.push(package);
        }
    }
    Ok(out)
}

/// A template name: free text, bounded, and never empty.
pub fn template_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 120 {
        return Err("A template name is 1 to 120 characters.".to_string());
    }
    if value.chars().any(char::is_control) {
        return Err("A template name must not contain control characters.".to_string());
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository_with(url: &str) -> RepositorySpec {
        RepositorySpec {
            name: "artgate".into(),
            url: url.into(),
            suite: Some("stable".into()),
            components: vec!["main".into()],
            signing_key: None,
            credential_ref: None,
        }
    }

    #[test]
    fn a_package_name_never_carries_shell_syntax() {
        assert!(package("nginx").is_ok());
        assert!(package("python3-pip").is_ok());
        assert!(package("libstdc++6").is_ok());
        assert!(package("nginx; curl evil.test").is_err());
        assert!(package("$(id)").is_err());
        assert!(package("").is_err());
    }

    #[test]
    fn a_repository_must_be_https_and_plain() {
        assert!(repository(&repository_with("https://packages.example.test/apt")).is_ok());
        assert!(repository(&repository_with("http://packages.example.test/apt")).is_err());
        assert!(repository(&repository_with("https://example.test/`id`")).is_err());
        assert!(repository(&repository_with("https://example.test/a b")).is_err());
    }

    #[test]
    fn a_private_credential_is_a_vault_locator_not_a_value() {
        let mut spec = repository_with("https://packages.example.test/apt");
        spec.credential_ref = Some("db://artgate-token".into());
        assert!(repository(&spec).is_ok());
        spec.credential_ref = Some("token-abc".into());
        assert!(repository(&spec).is_err());
    }

    #[test]
    fn duplicates_are_dropped_and_order_is_kept() {
        let packages = packages(&["vim".into(), "git".into(), "vim".into()]).expect("valid");
        assert_eq!(packages, vec!["vim".to_string(), "git".to_string()]);
    }
}
