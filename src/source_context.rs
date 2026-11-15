//! Bounded diagnostic symbols, never raw stack paths, messages, variables or request data.
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    pub module: String,
    pub function: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticContext {
    pub event_id: Option<String>,
    pub frames: Vec<Frame>,
    pub breadcrumb_categories: Vec<String>,
    pub limitation: String,
}
fn symbol(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str)
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.:$<>-".contains(&b))
        })
        .map(str::to_owned)
}
pub(crate) fn normalize(event: &Value) -> Option<DiagnosticContext> {
    if event.is_null() {
        return None;
    }
    let entries = event.get("entries").and_then(Value::as_array);
    let mut frames = Vec::new();
    let mut categories = Vec::new();
    for entry in entries.into_iter().flatten().take(20) {
        match entry.get("type").and_then(Value::as_str) {
            Some("exception") => {
                for exception in entry
                    .pointer("/data/values")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .take(10)
                {
                    for f in exception
                        .pointer("/stacktrace/frames")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .take(20)
                    {
                        if frames.len() == 20 {
                            break;
                        }
                        if let (Some(module), Some(function)) =
                            (symbol(f.get("module")), symbol(f.get("function")))
                        {
                            frames.push(Frame { module, function });
                        }
                    }
                }
            }
            Some("breadcrumbs") => {
                for crumb in entry
                    .pointer("/data/values")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .take(50)
                {
                    if let Some(category) =
                        crumb.get("category").and_then(Value::as_str).filter(|c| {
                            matches!(
                                *c,
                                "http"
                                    | "navigation"
                                    | "ui.click"
                                    | "console"
                                    | "query"
                                    | "default"
                            )
                        })
                        && categories.len() < 10
                        && !categories.iter().any(|c| c == category)
                    {
                        categories.push(category.to_owned());
                    }
                }
            }
            _ => {}
        }
    }
    Some(DiagnosticContext {
        event_id:event.get("eventID").and_then(Value::as_str).filter(|s|s.len()==32 && s.bytes().all(|b|b.is_ascii_hexdigit())).map(str::to_ascii_lowercase),
        frames, breadcrumb_categories:categories,
        limitation:"One representative event; bounded untrusted symbols only. Paths, code, variables, messages, users and request data excluded. Counts lack a traffic denominator; no causal or recovery conclusion.".into(),
    })
}
