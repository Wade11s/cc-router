use serde::Serialize;
use tauri::State;

use crate::error::AppResult;
use crate::settings::model::ProxyMode;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ProxyStatus {
    /// 兼容老前端字段: HTTP 端口 (HTTPS-only 模式下回退到 HTTPS 端口, 保留单值入口).
    pub port: u16,
    pub running: bool,
    pub mode: ProxyMode,
    /// HTTP listener 实际绑定端口, None=HTTP 未启用 (HTTPS-only 模式).
    pub http_port: Option<u16>,
    /// HTTPS listener 实际绑定端口, None=HTTPS 未启用 (HTTP-only 模式).
    pub https_port: Option<u16>,
    /// true: 监听 0.0.0.0; false: 仅 127.0.0.1.
    pub listen_all: bool,
    /// 客户端工具应连接的完整 base URL (含 scheme + port). 由 [`AppState::local_base_url`] 决定.
    /// 前端硬拼 URL 容易在 HTTPS-only / 端口冲突 +1 时出错, 改由后端给定唯一真相.
    pub base_url: String,
}

#[tauri::command]
pub async fn proxy_status(state: State<'_, AppState>) -> AppResult<ProxyStatus> {
    let http_port = *state.http_bound_port.read().await;
    let https_port = *state.https_bound_port.read().await;
    let (mode, listen_all) = {
        let g = state.settings.read().await;
        (g.proxy_mode, g.listen_all)
    };
    let primary = http_port.or(https_port).unwrap_or(0);
    let base_url = state.local_base_url().await;
    Ok(ProxyStatus {
        port: primary,
        running: primary != 0,
        mode,
        http_port,
        https_port,
        listen_all,
        base_url,
    })
}

/// Claude Code 接入 cc-router 的环境变量 (键, 值), 顺序即输出顺序。
/// 字段集与前端 `src/lib/recommendedClaudeCodeEnv.ts` 是同一份, 改一边要同步另一边。
/// `env_snippet` (接入指南) 与托盘「复制 Claude Code 环境变量」共用。
pub(crate) fn claude_code_env(base_url: &str, token: &str) -> Vec<(&'static str, String)> {
    let fixed: [(&'static str, &str); 11] = [
        ("API_TIMEOUT_MS", "3000000"),
        ("ANTHROPIC_MODEL", "model-opus"),
        ("ANTHROPIC_DEFAULT_FABLE_MODEL", "model-fable"),
        ("ANTHROPIC_DEFAULT_OPUS_MODEL", "model-opus"),
        ("ANTHROPIC_DEFAULT_SONNET_MODEL", "model-sonnet"),
        ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "model-haiku"),
        ("CLAUDE_CODE_SUBAGENT_MODEL", "model-opus"),
        ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
        ("CLAUDE_CODE_DISABLE_NONSTREAMING_FALLBACK", "1"),
        ("CLAUDE_CODE_ATTRIBUTION_HEADER", "0"),
        ("CLAUDE_CODE_EFFORT_LEVEL", "max"),
    ];
    let mut out = vec![
        ("ANTHROPIC_BASE_URL", base_url.to_string()),
        ("ANTHROPIC_AUTH_TOKEN", token.to_string()),
    ];
    out.extend(fixed.iter().map(|(k, v)| (*k, v.to_string())));
    out
}

/// POSIX shell 形态: `export K=V`, 值原样 (URL / token / 常量里都没有需要转义的字符)。
pub(crate) fn render_env_export(pairs: &[(&str, String)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("export {k}={v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// PowerShell 形态: `$env:K = "V"`。双引号串里 `` ` `` / `"` / `$` 要用反引号转义。
pub(crate) fn render_env_powershell(pairs: &[(&str, String)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| {
            let escaped: String = v
                .chars()
                .flat_map(|c| match c {
                    '`' | '"' | '$' => vec!['`', c],
                    _ => vec![c],
                })
                .collect();
            format!("$env:{k} = \"{escaped}\"")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tauri::command]
pub async fn env_snippet(state: State<'_, AppState>) -> AppResult<String> {
    let base_url = state.local_base_url().await;
    let token = state.settings.read().await.auth_token.clone();
    Ok(render_env_export(&claude_code_env(&base_url, &token)))
}

/// 网页界面设置页展示局域网访问地址用. 只列非回环 IPv4.
#[tauri::command]
pub async fn list_lan_addresses() -> AppResult<Vec<String>> {
    let ifaces = if_addrs::get_if_addrs()
        .map_err(|e| crate::error::AppError::internal(format!("枚举网卡失败: {e}")))?;
    let mut out: Vec<String> = ifaces
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.ip() {
            std::net::IpAddr::V4(v4) => Some(v4.to_string()),
            std::net::IpAddr::V6(_) => None,
        })
        .collect();
    out.sort();
    out.dedup();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 抽成 claude_code_env 之前 env_snippet 的原文, 锁住输出一个字符都不变。
    #[test]
    fn export_rendering_is_unchanged() {
        let got = render_env_export(&claude_code_env("http://127.0.0.1:23456", "tok"));
        assert_eq!(
            got,
            "export ANTHROPIC_BASE_URL=http://127.0.0.1:23456\n\
             export ANTHROPIC_AUTH_TOKEN=tok\n\
             export API_TIMEOUT_MS=3000000\n\
             export ANTHROPIC_MODEL=model-opus\n\
             export ANTHROPIC_DEFAULT_FABLE_MODEL=model-fable\n\
             export ANTHROPIC_DEFAULT_OPUS_MODEL=model-opus\n\
             export ANTHROPIC_DEFAULT_SONNET_MODEL=model-sonnet\n\
             export ANTHROPIC_DEFAULT_HAIKU_MODEL=model-haiku\n\
             export CLAUDE_CODE_SUBAGENT_MODEL=model-opus\n\
             export CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1\n\
             export CLAUDE_CODE_DISABLE_NONSTREAMING_FALLBACK=1\n\
             export CLAUDE_CODE_ATTRIBUTION_HEADER=0\n\
             export CLAUDE_CODE_EFFORT_LEVEL=max"
        );
    }

    #[test]
    fn powershell_rendering_quotes_and_escapes() {
        let got = render_env_powershell(&[("A", "http://x:1".into()), ("B", "a$b\"c`d".into())]);
        assert_eq!(got, "$env:A = \"http://x:1\"\n$env:B = \"a`$b`\"c``d\"");
    }
}
