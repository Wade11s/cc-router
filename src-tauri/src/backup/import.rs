//! Import: preview, secret binding checks, and a pure `plan`; `apply` (Task 6) writes it.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::format::{export_to_row, header_field, ExportFile, ExportSubscription, SecretsPlain, FIELD_API_KEY};
use crate::error::{AppError, AppResult};
use crate::subscription::model::SubscriptionRow;
use crate::virtual_model::model::{RoutingMode, VirtualModelName};

pub struct LocalState {
    pub existing_ids: HashSet<Uuid>,
    pub bindings: HashMap<VirtualModelName, Vec<Uuid>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewStatus {
    New,
    SkipExistingId,
    SkipOauth,
}

#[derive(Debug, Serialize)]
pub struct PreviewItem {
    pub id: String,
    pub display_name: String,
    pub provider_id: String,
    pub provider_display_name: String,
    pub provider_icon: String,
    pub status: PreviewStatus,
    pub has_api_key: bool,
    pub redacted_headers: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportPreview {
    pub app_version: String,
    pub exported_at: i64,
    pub has_secrets: bool,
    pub local_subscription_count: usize,
    pub subscriptions: Vec<PreviewItem>,
}

fn status_of(sub: &ExportSubscription, local: &LocalState) -> PreviewStatus {
    if local.existing_ids.contains(&sub.id) {
        PreviewStatus::SkipExistingId
    } else if sub.is_oauth() {
        PreviewStatus::SkipOauth
    } else {
        PreviewStatus::New
    }
}

pub fn preview(file: &ExportFile, local: &LocalState) -> ImportPreview {
    ImportPreview {
        app_version: file.app_version.clone(),
        exported_at: file.exported_at.timestamp_millis(),
        has_secrets: file.secrets.is_some(),
        local_subscription_count: local.existing_ids.len(),
        subscriptions: file
            .subscriptions
            .iter()
            .map(|s| PreviewItem {
                id: s.id.to_string(),
                display_name: s.display_name.clone(),
                provider_id: s.provider_id.clone(),
                provider_display_name: s.provider_display_name.clone(),
                provider_icon: s.provider_icon.clone(),
                status: status_of(s, local),
                has_api_key: s.has_api_key,
                redacted_headers: s.redacted_headers.clone(),
            })
            .collect(),
    }
}

#[derive(Debug, Default)]
pub struct ResolvedSecrets {
    pub auth_token: Option<String>,
    pub api_keys: HashMap<Uuid, String>,
    pub headers: HashMap<Uuid, BTreeMap<String, String>>,
}

/// Every referenced secret must be bound to the same subscription, field and destinations as
/// the (untrusted) plaintext part claims; each ref is consumed once so refs cannot be shared.
pub fn resolve_secrets(file: &ExportFile, plain: SecretsPlain) -> AppResult<ResolvedSecrets> {
    let tampered = || AppError::BadRequest("文件被修改过, 已拒绝导入密钥".into());
    let SecretsPlain { auth_token, mut items } = plain;
    let mut out = ResolvedSecrets {
        auth_token: auth_token.filter(|t| !t.is_empty()),
        ..Default::default()
    };
    for sub in &file.subscriptions {
        let Some(refs) = &sub.secret_refs else { continue };
        let destinations = sub.key_destinations();
        let mut take = |r: &str, field: String| -> AppResult<String> {
            let item = items.remove(r).ok_or_else(tampered)?;
            let b = &item.bind;
            if b.subscription_id != sub.id || b.field != field || b.destinations != destinations {
                return Err(tampered());
            }
            Ok(item.value)
        };
        if let Some(r) = &refs.api_key {
            let v = take(r, FIELD_API_KEY.into())?;
            out.api_keys.insert(sub.id, v);
        }
        for (name, r) in &refs.headers {
            let v = take(r, header_field(name))?;
            out.headers.entry(sub.id).or_default().insert(name.clone(), v);
        }
    }
    Ok(out)
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    pub imported: usize,
    pub skipped_existing: usize,
    pub skipped_oauth: Vec<String>,
    pub disabled_missing_key: Vec<String>,
    pub token_imported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_error: Option<String>,
}

#[derive(Debug)]
pub struct ImportPlan {
    pub inserts: Vec<SubscriptionRow>,
    /// Full new binding list for each virtual model that changes.
    pub bindings: Vec<(VirtualModelName, Vec<Uuid>)>,
    pub modes: Vec<(VirtualModelName, RoutingMode)>,
    pub report: ImportReport,
}

pub fn plan(
    file: &ExportFile,
    secrets: Option<&ResolvedSecrets>,
    local: &LocalState,
    now: DateTime<Utc>,
) -> ImportPlan {
    let mut report = ImportReport::default();
    let mut inserts = Vec::new();
    let mut new_ids = HashSet::new();

    for sub in &file.subscriptions {
        match status_of(sub, local) {
            PreviewStatus::SkipExistingId => {
                report.skipped_existing += 1;
                continue;
            }
            PreviewStatus::SkipOauth => {
                report.skipped_oauth.push(sub.display_name.clone());
                continue;
            }
            PreviewStatus::New => {}
        }
        let api_key = secrets.and_then(|s| s.api_keys.get(&sub.id)).cloned();
        let recovered = secrets.and_then(|s| s.headers.get(&sub.id));
        let mut headers = sub.required_headers.clone();
        let mut missing = sub.has_api_key && api_key.is_none();
        for name in &sub.redacted_headers {
            match recovered.and_then(|h| h.get(name)) {
                Some(v) => {
                    headers.insert(name.clone(), v.clone());
                }
                None => {
                    // Keep the header name so the edit page shows what still needs filling in.
                    headers.insert(name.clone(), String::new());
                    missing = true;
                }
            }
        }
        if missing {
            report.disabled_missing_key.push(sub.display_name.clone());
        }
        let enabled = sub.enabled && !missing;
        inserts.push(export_to_row(sub, api_key.unwrap_or_default(), headers, enabled, now));
        new_ids.insert(sub.id);
    }
    report.imported = inserts.len();

    let mut bindings = Vec::new();
    let mut modes = Vec::new();
    for vm in &file.virtual_models {
        let current = local.bindings.get(&vm.name).cloned().unwrap_or_default();
        let mut next = current.clone();
        for id in &vm.subscription_ids {
            if new_ids.contains(id) && !next.contains(id) {
                next.push(*id);
            }
        }
        if next != current {
            if current.is_empty() {
                modes.push((vm.name, vm.mode));
            }
            bindings.push((vm.name, next));
        }
    }

    ImportPlan { inserts, bindings, modes, report }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::crypto::{open, KdfCost};
    use crate::backup::export::{build_export, SecretOptions, VirtualModelSnapshot};
    use crate::provider::model::AuthType;
    use crate::subscription::model::SubscriptionRow;

    const PW: &str = "0123456789";

    struct Fixture {
        rows: Vec<SubscriptionRow>,
        file: ExportFile,
    }

    /// rows[0]: 有 Key + 敏感头, rows[1]: 本来没 Key (本地模型), rows[2]: ChatGPT OAuth
    fn fixture(with_secrets: bool) -> Fixture {
        let mut a = SubscriptionRow::test_fixture("zhipu", "cn");
        a.display_name = "A".into();
        a.base_url = "https://a.example".into();
        a.api_key = "sk-a".into();
        a.required_headers.insert("x-relay-key".into(), "hk".into());
        let mut b = SubscriptionRow::test_fixture("custom", "custom");
        b.display_name = "B".into();
        b.api_key = String::new();
        let mut c = SubscriptionRow::test_fixture("openai_codex", "default");
        c.display_name = "C".into();
        c.auth_type = AuthType::ChatgptOauth;
        c.api_key = String::new();
        let rows = vec![a, b, c];
        let vms = vec![VirtualModelSnapshot {
            name: VirtualModelName::Opus,
            mode: RoutingMode::Sticky,
            subscription_ids: rows.iter().map(|r| r.id).collect(),
        }];
        let secrets = with_secrets.then(|| SecretOptions { password: PW, auth_token: "tok", cost: KdfCost::FAST });
        let file = build_export(&rows, &vms, secrets, "6.1.0", Utc::now()).unwrap();
        Fixture { rows, file }
    }

    fn empty_local() -> LocalState {
        LocalState { existing_ids: HashSet::new(), bindings: HashMap::new() }
    }

    fn resolved(f: &Fixture) -> ResolvedSecrets {
        let plain = open(f.file.secrets.as_ref().unwrap(), PW).unwrap();
        resolve_secrets(&f.file, plain).unwrap()
    }

    #[test]
    fn preview_marks_statuses() {
        let f = fixture(false);
        let mut local = empty_local();
        local.existing_ids.insert(f.rows[1].id);
        let p = preview(&f.file, &local);
        let status = |id: Uuid| p.subscriptions.iter().find(|s| s.id == id.to_string()).unwrap().status;
        assert_eq!(status(f.rows[0].id), PreviewStatus::New);
        assert_eq!(status(f.rows[1].id), PreviewStatus::SkipExistingId);
        assert_eq!(status(f.rows[2].id), PreviewStatus::SkipOauth);
        assert!(!p.has_secrets);
        assert_eq!(p.local_subscription_count, 1);
    }

    #[test]
    fn full_import_with_secrets() {
        let f = fixture(true);
        let s = resolved(&f);
        assert_eq!(s.auth_token.as_deref(), Some("tok"));
        let plan = plan(&f.file, Some(&s), &empty_local(), Utc::now());
        assert_eq!(plan.report.imported, 2);
        assert_eq!(plan.report.skipped_oauth, vec!["C".to_string()]);
        assert!(plan.report.disabled_missing_key.is_empty());
        let a = plan.inserts.iter().find(|r| r.id == f.rows[0].id).unwrap();
        assert_eq!(a.api_key, "sk-a");
        assert_eq!(a.required_headers.get("x-relay-key").map(String::as_str), Some("hk"));
        assert!(a.enabled);
        assert_eq!(plan.bindings, vec![(VirtualModelName::Opus, vec![f.rows[0].id, f.rows[1].id])], "OAuth 不进绑定");
        assert_eq!(plan.modes, vec![(VirtualModelName::Opus, RoutingMode::Sticky)], "本机为空时采用文件的模式");
    }

    #[test]
    fn without_secrets_keyed_subscriptions_are_disabled_but_keyless_ones_are_not() {
        let f = fixture(false);
        let plan = plan(&f.file, None, &empty_local(), Utc::now());
        assert_eq!(plan.report.disabled_missing_key, vec!["A".to_string()]);
        let a = plan.inserts.iter().find(|r| r.id == f.rows[0].id).unwrap();
        assert!(!a.enabled);
        assert_eq!(a.api_key, "");
        assert_eq!(a.required_headers.get("x-relay-key").map(String::as_str), Some(""), "保留头名, 值置空");
        let b = plan.inserts.iter().find(|r| r.id == f.rows[1].id).unwrap();
        assert!(b.enabled, "本来就没 Key 的订阅保持原启停状态");
    }

    #[test]
    fn existing_ids_are_skipped_and_bindings_only_append_new_ones() {
        let f = fixture(false);
        let other = Uuid::new_v4();
        let mut local = empty_local();
        local.existing_ids.insert(f.rows[1].id);
        local.existing_ids.insert(other);
        local.bindings.insert(VirtualModelName::Opus, vec![other]);
        let plan = plan(&f.file, None, &local, Utc::now());
        assert_eq!(plan.report.skipped_existing, 1);
        assert_eq!(plan.report.imported, 1);
        assert_eq!(plan.bindings, vec![(VirtualModelName::Opus, vec![other, f.rows[0].id])]);
        assert!(plan.modes.is_empty(), "本机已有绑定时不改模式");
    }

    #[test]
    fn reimporting_the_same_file_changes_nothing() {
        let f = fixture(false);
        let mut local = empty_local();
        for r in &f.rows {
            local.existing_ids.insert(r.id);
        }
        local.bindings.insert(VirtualModelName::Opus, vec![f.rows[0].id, f.rows[1].id]);
        let plan = plan(&f.file, None, &local, Utc::now());
        assert!(plan.inserts.is_empty());
        assert!(plan.bindings.is_empty());
        assert!(plan.modes.is_empty());
    }

    #[test]
    fn tampered_destination_is_rejected() {
        let mut f = fixture(true);
        let plain = open(f.file.secrets.as_ref().unwrap(), PW).unwrap();
        let a = f.file.subscriptions.iter_mut().find(|s| s.display_name == "A").unwrap();
        a.base_url = "https://evil.example".into();
        assert!(resolve_secrets(&f.file, plain).unwrap_err().to_string().contains("被修改过"));
    }

    #[test]
    fn tampered_model_discovery_url_is_rejected() {
        let mut f = fixture(true);
        let plain = open(f.file.secrets.as_ref().unwrap(), PW).unwrap();
        let a = f.file.subscriptions.iter_mut().find(|s| s.display_name == "A").unwrap();
        a.model_discovery.url = Some("https://evil.example/models".into());
        assert!(resolve_secrets(&f.file, plain).is_err());
    }

    #[test]
    fn moving_a_secret_ref_to_another_subscription_is_rejected() {
        let mut f = fixture(true);
        let plain = open(f.file.secrets.as_ref().unwrap(), PW).unwrap();
        let refs = f.file.subscriptions.iter().find(|s| s.display_name == "A").unwrap().secret_refs.clone();
        let b = f.file.subscriptions.iter_mut().find(|s| s.display_name == "B").unwrap();
        b.base_url = "https://a.example".into(); // same destination, different subscription id
        b.secret_refs = refs;
        assert!(resolve_secrets(&f.file, plain).is_err());
    }
}
