//! Shared provider-header policy. Configuration carries names only; values
//! are resolved from credentials for a single request and never serialized.
use std::collections::{BTreeMap, BTreeSet};

use crate::error::{AppError, AppResult};

pub fn normalize_names(names: &[String]) -> AppResult<Vec<String>> {
    let mut seen = BTreeSet::new();
    names
        .iter()
        .map(|name| {
            let name = name.trim().to_ascii_lowercase();
            if reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_err() {
                return Err(AppError::validation(
                    "invalid_header_name",
                    "Enter a valid HTTP header name.",
                ));
            }
            if matches!(
                name.as_str(),
                "host"
                    | "content-length"
                    | "transfer-encoding"
                    | "connection"
                    | "keep-alive"
                    | "te"
                    | "trailer"
                    | "upgrade"
                    | "proxy-authorization"
                    | "proxy-authenticate"
                    | "content-type"
                    | "accept"
            ) {
                return Err(AppError::validation(
                    "reserved_header_name",
                    "This header is managed by Goal.",
                ));
            }
            if !seen.insert(name.clone()) {
                return Err(AppError::validation(
                    "duplicate_header_name",
                    "Header names must be unique, ignoring case.",
                ));
            }
            Ok(name)
        })
        .collect()
}

fn validate_value(value: &str) -> AppResult<()> {
    if value.is_empty() {
        return Err(AppError::validation(
            "missing_header_value",
            "Enter a value for the new header.",
        ));
    }
    // Match the UI policy and avoid implicit UTF-8/header-byte ambiguity.
    if !value
        .bytes()
        .all(|b| b == b'\t' || (b' '..=b'~').contains(&b))
    {
        return Err(AppError::validation(
            "invalid_header_value",
            "Header values must contain printable ASCII or tabs, without line breaks.",
        ));
    }
    Ok(())
}

pub fn validate_headers(headers: &[(String, String)]) -> AppResult<()> {
    normalize_names(
        &headers
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>(),
    )?;
    for (_, value) in headers {
        validate_value(value)?;
    }
    Ok(())
}

/// Retain only the selected names, and replace only explicitly supplied
/// values. A renamed header is new, so it requires a value.
pub fn resolve_headers(
    names: &[String],
    saved: &BTreeMap<String, String>,
    replacements: &BTreeMap<String, String>,
) -> AppResult<BTreeMap<String, String>> {
    let names = normalize_names(names)?;
    let replacement_names = replacements.keys().cloned().collect::<Vec<_>>();
    let normalized = normalize_names(&replacement_names)?;
    let mut updates = BTreeMap::new();
    for ((_, value), name) in replacements.iter().zip(normalized) {
        if !names.contains(&name) {
            return Err(AppError::validation(
                "unknown_header_value",
                "A value was provided for an unselected header.",
            ));
        }
        validate_value(value)?;
        updates.insert(name, value.clone());
    }
    let mut result = BTreeMap::new();
    for name in names {
        let value = updates
            .get(&name)
            .or_else(|| saved.get(&name))
            .ok_or_else(|| {
                AppError::validation(
                    "missing_header_value",
                    "A header value is missing. Enter it again in AI settings.",
                )
            })?;
        validate_value(value)?;
        crate::logging::register_secret(value);
        result.insert(name, value.clone());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code<T>(result: AppResult<T>) -> String {
        match result {
            Err(AppError::Validation { code, .. }) => code,
            _ => panic!("expected validation error"),
        }
    }

    #[test]
    fn names_and_values_share_one_policy() {
        assert_eq!(
            normalize_names(&[" Authorization ".into(), "Anthropic-Version".into()]).unwrap(),
            ["authorization", "anthropic-version"]
        );
        assert_eq!(
            code(normalize_names(&["X-Key".into(), "x-key".into()])),
            "duplicate_header_name"
        );
        for name in ["", "bad name", "x:key", "头部", "X\r\nTest"] {
            assert_eq!(code(normalize_names(&[name.into()])), "invalid_header_name");
        }
        for name in ["Host", "Content-Type", "TRANSFER-ENCODING", "Accept"] {
            assert_eq!(
                code(normalize_names(&[name.into()])),
                "reserved_header_name"
            );
        }
        for value in ["a\r\nb", "secret\0", "a\x7f", "中文"] {
            let error = validate_headers(&[("x-test".into(), value.into())]).unwrap_err();
            assert!(!error.to_string().contains(value));
            assert_eq!(code::<()>(Err(error)), "invalid_header_value");
        }
        validate_headers(&[("x-test".into(), "  value\t ".into())]).unwrap();
    }

    #[test]
    fn replacement_retention_deletion_and_missing_value_are_explicit() {
        let saved = BTreeMap::from([
            ("x-route".into(), "old".into()),
            ("x-remove".into(), "gone".into()),
        ]);
        let names = vec!["X-Route".into(), "x-new".into()];
        let updates = BTreeMap::from([("X-New".into(), " new ".into())]);
        assert_eq!(
            resolve_headers(&names, &saved, &updates).unwrap(),
            BTreeMap::from([
                ("x-route".into(), "old".into()),
                ("x-new".into(), " new ".into())
            ])
        );
        assert_eq!(
            code(resolve_headers(&names, &saved, &BTreeMap::new())),
            "missing_header_value"
        );
        assert_eq!(
            code(resolve_headers(&[], &saved, &updates)),
            "unknown_header_value"
        );
        assert!(resolve_headers(&[], &saved, &BTreeMap::new())
            .unwrap()
            .is_empty());
    }
}
