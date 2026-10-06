use std::collections::HashMap;

use tracing::{error, info, warn};

use crate::error::{AppError, AppResult};
use crate::provider::model::Provider;
use crate::provider::schema;

// build.rs 扫描 `providers/*.yaml` 生成: `pub static EMBEDDED_PROVIDERS: &[(文件名, 内容)]`。
// yaml 编进二进制而不是走 bundle resource —— 内置 provider 只在启动时读一次、运行时不写,
// 没有留在安装目录的理由, 还省掉了「加 provider 要登记 tauri.conf.json」这一步。
include!(concat!(env!("OUT_DIR"), "/embedded_providers.rs"));

/// 逐个解析并校验内嵌的 provider yaml。
/// 单个文件失败不阻塞其他 provider 加载；id 冲突是 fatal。
/// (两种情况都由本文件的单测在 `cargo test` 阶段拦住, 正常不会在用户机器上发生。)
pub fn load_all() -> AppResult<HashMap<String, Provider>> {
    if let Err(e) = schema::compile() {
        warn!(?e, "provider schema 编译失败, 将跳过 schema 校验");
    }

    let mut map: HashMap<String, Provider> = HashMap::new();
    for (file, raw) in EMBEDDED_PROVIDERS {
        match parse_single(raw) {
            Ok(provider) => {
                if map.contains_key(&provider.id) {
                    return Err(AppError::internal(format!(
                        "provider id '{}' 冲突 (文件: {file})",
                        provider.id
                    )));
                }
                info!(provider = %provider.id, %file, "provider loaded");
                map.insert(provider.id.clone(), provider);
            }
            Err(e) => {
                error!(?e, %file, "provider 加载失败, 跳过");
            }
        }
    }

    Ok(map)
}

fn parse_single(raw: &str) -> AppResult<Provider> {
    let as_yaml: serde_yaml::Value = serde_yaml::from_str(raw)?;
    // 转为 JSON 以便 schema 校验
    let as_json = serde_json::to_value(&as_yaml)?;
    let _ = schema::validate(&as_json);
    let provider: Provider = serde_yaml::from_value(as_yaml)?;
    validate_semantics(&provider)?;
    Ok(provider)
}

/// schema 表达不了的跨字段约束。失败即该 yaml 加载失败 (load_all warn + 跳过, 单测在 CI 拦住)。
fn validate_semantics(p: &Provider) -> AppResult<()> {
    use crate::provider::model::{AuthType, EndpointProtocol};
    for e in &p.endpoints {
        if e.protocol == EndpointProtocol::Systemone && p.auth.auth_type != AuthType::ApiKey {
            return Err(AppError::internal(format!(
                "endpoint '{}': protocol systemone 只允许出现在 auth.type = api_key 的 provider 下",
                e.id
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::model::LocalizedText;

    /// yaml 已经编进二进制, 「解析失败 / id 冲突」不再需要等到用户机器上启动才发现。
    #[test]
    fn every_embedded_provider_parses_and_ids_are_unique() {
        assert!(!EMBEDDED_PROVIDERS.is_empty());
        let mut seen: HashMap<String, &str> = HashMap::new();
        for (file, raw) in EMBEDDED_PROVIDERS {
            let p = parse_single(raw).unwrap_or_else(|e| panic!("{file} 解析失败: {e}"));
            if let Some(prev) = seen.insert(p.id.clone(), file) {
                panic!("provider id '{}' 冲突: {prev} 与 {file}", p.id);
            }
            // schema 不强校验这条软约束, 写错了前端新建向导会默认选不中任何 endpoint。
            if let Some(default) = p.default_endpoint.as_deref() {
                assert!(
                    p.endpoints.iter().any(|e| e.id == default),
                    "{file}: default_endpoint '{default}' 不在 endpoints[].id 中"
                );
            }
        }
        // load_all 对单个失败是 warn + 跳过, 数量相等才说明一个都没被吞掉。
        assert_eq!(load_all().unwrap().len(), EMBEDDED_PROVIDERS.len());
    }

    /// 汉字 + 假名。全角标点不算: 英文译文里偶尔出现也不是漏翻。
    fn has_cjk(s: &str) -> bool {
        s.chars().any(|c| matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}'))
    }

    /// 一个 provider 里所有上屏的多语字段, 带着 yaml 路径方便报错。
    fn texts(p: &Provider) -> Vec<(String, &LocalizedText)> {
        let mut out = vec![("display_name".to_string(), &p.display_name)];
        out.extend(p.description.iter().map(|t| ("description".to_string(), t)));
        out.extend(p.compatibility_notes.iter().map(|t| ("compatibility_notes".to_string(), t)));
        for e in &p.endpoints {
            out.push((format!("endpoints[{}].label", e.id), &e.label));
            out.extend(e.description.iter().map(|t| (format!("endpoints[{}].description", e.id), t)));
        }
        out
    }

    /// 界面上没有回退: 含中文的字段必须写成 `{zh, en, ja}`, 而且英文里不能再有中文、日文不能照抄中文。
    /// 纯字符串写法只留给三语都一样的品牌名。
    #[test]
    fn cjk_text_is_translated() {
        let mut problems = Vec::new();
        for (file, raw) in EMBEDDED_PROVIDERS {
            let p = parse_single(raw).unwrap_or_else(|e| panic!("{file} 解析失败: {e}"));
            for (path, t) in texts(&p) {
                if has_cjk(&t.en) {
                    problems.push(format!("{file} {path}: en 含中日文 ({:?}), 纯字符串写法?", t.en));
                } else if has_cjk(&t.zh) && t.ja == t.zh {
                    problems.push(format!("{file} {path}: ja 与 zh 相同 ({:?})", t.ja));
                }
            }
        }
        assert!(problems.is_empty(), "{} 处未翻译:\n{}", problems.len(), problems.join("\n"));
    }

    #[test]
    fn localized_text_accepts_string_or_all_three_languages() {
        let same: LocalizedText = serde_yaml::from_str("DeepSeek").unwrap();
        assert_eq!(same, LocalizedText::same("DeepSeek"));

        let per: LocalizedText = serde_yaml::from_str("{ zh: 国内版, en: China, ja: 中国版 }").unwrap();
        assert_eq!((per.zh.as_str(), per.en.as_str(), per.ja.as_str()), ("国内版", "China", "中国版"));

        // 报错要能指出是哪个键, 否则加 provider 的人只看到一句 "did not match any variant"。
        let missing = serde_yaml::from_str::<LocalizedText>("{ zh: 国内版, en: China }").unwrap_err();
        assert!(missing.to_string().contains("`ja`"), "{missing}");
        let typo = serde_yaml::from_str::<LocalizedText>("{ zh: a, en: b, ja: c, jp: d }").unwrap_err();
        assert!(typo.to_string().contains("`jp`"), "{typo}");
    }

    const MINIMAL: &str = r#"
id: demo
display_name: Demo
compatibility: untested
endpoints:
  - id: chat
    label: Chat
    base_url: "https://example.invalid"
    messages_path: "/v1/messages"
auth: { type: api_key, header_name: Authorization, header_format: bearer }
"#;

    #[test]
    fn endpoint_protocol_defaults_to_messages() {
        let p = parse_single(MINIMAL).unwrap();
        assert_eq!(p.endpoints[0].protocol, crate::provider::model::EndpointProtocol::Messages);
        assert!(p.endpoints[0].example_models.is_empty());
    }

    #[test]
    fn systemone_endpoint_parses_with_its_own_example_models() {
        let yaml = MINIMAL.replace(
            "    messages_path: \"/v1/messages\"\n",
            "    messages_path: \"/v1/systemone\"\n    protocol: systemone\n    example_models: [\"jev-latest\"]\n",
        );
        let p = parse_single(&yaml).unwrap();
        assert_eq!(p.endpoints[0].protocol, crate::provider::model::EndpointProtocol::Systemone);
        assert_eq!(p.endpoints[0].example_models, vec!["jev-latest".to_string()]);
    }

    /// 翻译类 provider 的端点标 systemone 会让 dispatch 不知道走哪条路, 必须在加载时拒绝。
    #[test]
    fn systemone_endpoint_requires_api_key_auth() {
        let yaml = MINIMAL
            .replace("    messages_path: \"/v1/messages\"\n", "    messages_path: \"/v1/systemone\"\n    protocol: systemone\n")
            .replace("type: api_key", "type: gemini_api_key");
        let err = parse_single(&yaml).unwrap_err();
        assert!(err.to_string().contains("systemone"), "{err}");
    }

    /// 三家 Jev 上游的 systemone 端点都在, 且 typesafe 只有 systemone 端点。
    #[test]
    fn builtin_systemone_endpoints_exist() {
        use crate::provider::model::EndpointProtocol::Systemone;
        let all = load_all().unwrap();
        let ep = |pid: &str, eid: &str| {
            all.get(pid).and_then(|p| p.endpoint(eid)).unwrap_or_else(|| panic!("{pid}/{eid} 缺失")).clone()
        };
        let ollama = ep("ollama", "localhost_systemone");
        assert_eq!((ollama.protocol, ollama.messages_path.as_str()), (Systemone, "/v1/systemone"));
        assert_eq!(ollama.example_models, vec!["clef-flash".to_string()]);
        let or = ep("openrouter", "systemone");
        assert_eq!((or.protocol, or.base_url.as_str()), (Systemone, "https://openrouter.ai/api"));
        let ts = all.get("typesafe").expect("typesafe.yaml 缺失");
        assert!(ts.endpoints.iter().all(|e| e.protocol == Systemone));
        assert_eq!(ts.auth.header_format, crate::provider::model::AuthHeaderFormat::Bearer);
        // 原有对话端点不受影响
        assert_eq!(ep("ollama", "localhost").protocol, crate::provider::model::EndpointProtocol::Messages);
    }
}
