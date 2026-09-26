//! cc-router-tui 入口: 解析参数 → 找到并连上桌面 app → 进界面 (或 `--check` 只打印状态)。
//! 连接失败的提示在进入备用屏幕**之前**打印到 stderr, 这样用户退出后还看得到。

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use cc_router_tui::app::{App, AppOptions};
use cc_router_tui::client::discovery::{default_data_dir, read_runtime, Platform};
use cc_router_tui::client::dto::{ProxyStatus, Settings, Subscription};
use cc_router_tui::client::{commands, Client, ClientError};
use cc_router_tui::format::Tz;
use cc_router_tui::i18n::{strings, Lang};
use cc_router_tui::runtime;
use cc_router_tui::theme::{ColorMode, Theme};
use serde_json::json;

const HELP: &str = "\
cc-router-tui — cc-router 的终端界面

用法: cc-router-tui [选项]
不带参数运行即进入界面 (需要 cc-router 桌面 app 正在运行, 且已在 设置 → 安全与访问 → 终端界面 打开开关)。

选项:
  --check            连接正在运行的 cc-router 并打印状态, 然后退出
  --data-dir <路径>  指定 cc-router 的数据目录 (默认按系统规则查找)
  --no-fx            关闭动效 (也可以设环境变量 CCR_TUI_NO_FX=1)
  -V, --version      打印版本
  -h, --help         打印本帮助
";

#[derive(Debug, Default, PartialEq, Eq)]
struct Args {
    data_dir: Option<PathBuf>,
    check: bool,
    no_fx: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Parsed {
    Run(Args),
    Help,
    Version,
    /// 参数有误; 内容是要打印的那句话。
    Invalid(String),
}

fn parse_args(mut argv: impl Iterator<Item = String>) -> Parsed {
    let mut args = Args::default();
    while let Some(a) = argv.next() {
        match a.as_str() {
            "-h" | "--help" => return Parsed::Help,
            "-V" | "--version" => return Parsed::Version,
            "--check" => args.check = true,
            "--no-fx" => args.no_fx = true,
            "--data-dir" => match argv.next() {
                Some(p) => args.data_dir = Some(PathBuf::from(p)),
                None => return Parsed::Invalid("--data-dir 需要一个路径".into()),
            },
            other => return Parsed::Invalid(format!("未知参数: {other}")),
        }
    }
    Parsed::Run(args)
}

/// 给人看的一句话 + 下一步该做什么。
///
/// `_lang`: 连接前语言 (见 [`pre_connect_lang`])。本任务只接线, 文案仍是硬编码中文——
/// 下一期把这几行搬进 `Strings` 后, 这个参数才会真正决定语言。
fn explain(err: &ClientError, _lang: Lang) -> String {
    match err {
        ClientError::Discovery(e) => format!("{e}\n请先启动 cc-router 桌面 app。"),
        ClientError::NotRunning => "cc-router 未在运行。请先启动桌面 app。".into(),
        ClientError::Disabled => "终端界面未启用。请在桌面 app 的 设置 → 安全与访问 → 终端界面 打开开关。".into(),
        other => other.to_string(),
    }
}

/// `ui` 里两类完全不同的失败: 连不上桌面 app (`ClientError`, 走 `explain`) vs 本地终端本身
/// 初始化不了 (比如没有 tty)。故意不把后者塞进 `ClientError::Transport`——那条分支的 Display
/// 是「网络错误: …」, 会让「请在真正的终端窗口里运行」被误报成网络问题。
enum Failure {
    Client(ClientError),
    Terminal(std::io::Error),
}

impl From<ClientError> for Failure {
    fn from(e: ClientError) -> Self {
        Failure::Client(e)
    }
}

/// 终端初始化失败 (没有可用 tty 等) 时给人看的一句话。
///
/// `_lang`: 见 [`explain`] 同一条注释——本任务只接线, 文案不变。
fn terminal_failure_message(err: &std::io::Error, _lang: Lang) -> String {
    format!("无法初始化终端: {err}\n请在真正的终端窗口里运行 cc-router-tui。")
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

fn resolve_data_dir(args: &Args) -> Result<PathBuf, ClientError> {
    match &args.data_dir {
        Some(d) => Ok(d.clone()),
        None => Ok(default_data_dir(Platform::current(), env)?),
    }
}

fn connect(args: &Args) -> Result<Client, ClientError> {
    Client::connect(&resolve_data_dir(args)?)
}

/// 连接建立 (甚至尝试连接) 之前能用的语言, 给「未运行 / 未启用 / 数据目录找不到」这类
/// 连接前提示用——这些错误发生在拿到 `Settings::preferred_language` 之前, 界面语言的
/// 那条解析路径够不到它们。尽力读一次 runtime.json 拿桌面端下发的 `system_locale`, 读不到
/// 或数据目录本身解析不出来都不算错误, 只是退回环境变量探测 (`Lang::resolve` 的 `None` 分支)。
fn pre_connect_lang(args: &Args) -> Lang {
    let system_locale = resolve_data_dir(args).ok().and_then(|dir| read_runtime(&dir).ok()).and_then(|info| info.system_locale);
    Lang::resolve("system", system_locale.as_deref(), env)
}

async fn check(client: Client) -> Result<(), ClientError> {
    let status: ProxyStatus = client.call(commands::PROXY_STATUS, json!({})).await?;
    let settings: Settings = client.call(commands::GET_SETTINGS, json!({})).await?;
    let subs: Vec<Subscription> = client.call(commands::LIST_SUBSCRIPTIONS, json!({})).await?;
    let _events = client.events().await?; // 只验证事件流能建立
    let rt = client.runtime().await;

    let dispatchable = subs.iter().filter(|s| s.is_dispatchable).count();
    println!("已连接 cc-router {} (pid {})", rt.app_version, rt.pid);
    println!("  地址     {}", status.base_url);
    println!("  模式     {}{}", status.mode, if status.listen_all { " · 监听 0.0.0.0" } else { "" });
    println!("  订阅     {} 个, {} 个可调度", subs.len(), dispatchable);
    println!("  语言     {}", settings.preferred_language);
    println!("  事件流   正常");
    if rt.app_version != env!("CARGO_PKG_VERSION") {
        println!("\n注意: TUI 版本 {} 与 app 版本 {} 不一致。", env!("CARGO_PKG_VERSION"), rt.app_version);
    }
    Ok(())
}

async fn ui(client: Client, no_fx: bool) -> Result<(), Failure> {
    // 进界面前先调一次: 既拿到语言设置, 也把「未运行 / 未启用」这类错误挡在备用屏幕之外。
    let settings: Settings = client.call(commands::GET_SETTINGS, json!({})).await?;
    // runtime.json 早在 `Client::connect` 时就读过了 (discovery), 这里只是取出已经拿到的
    // `system_locale`——不是重新触发一次 IO。
    let rt = client.runtime().await;
    let theme = Theme::new(ColorMode::detect(env));
    let fx_enabled = !no_fx && env("CCR_TUI_NO_FX").is_none_or(|v| v.is_empty() || v == "0") && theme.supports_fx();
    let app = App::new(AppOptions {
        strings: strings(Lang::resolve(&settings.preferred_language, rt.system_locale.as_deref(), env)),
        theme,
        fx_enabled,
        now_ms: runtime::unix_ms(),
        tui_version: env!("CARGO_PKG_VERSION"),
        tz: Tz::Local,
    });
    runtime::run(Arc::new(client), app).await.map_err(Failure::Terminal)
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Parsed::Run(a) => a,
        Parsed::Help => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Parsed::Version => {
            println!("cc-router-tui {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Parsed::Invalid(msg) => {
            eprintln!("{msg}\n\n{HELP}");
            return ExitCode::from(2);
        }
    };
    let lang = pre_connect_lang(&args);
    let result: Result<(), Failure> = match connect(&args) {
        Ok(client) if args.check => check(client).await.map_err(Failure::Client),
        Ok(client) => ui(client, args.no_fx).await,
        Err(e) => Err(Failure::Client(e)),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Client(e)) => {
            eprintln!("{}", explain(&e, lang));
            ExitCode::FAILURE
        }
        Err(Failure::Terminal(e)) => {
            eprintln!("{}", terminal_failure_message(&e, lang));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Parsed {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_arguments_means_run_the_ui() {
        assert_eq!(parse(&[]), Parsed::Run(Args::default()));
    }

    #[test]
    fn flags_combine_in_any_order() {
        assert_eq!(
            parse(&["--no-fx", "--data-dir", "/tmp/x", "--check"]),
            Parsed::Run(Args { data_dir: Some("/tmp/x".into()), check: true, no_fx: true })
        );
    }

    #[test]
    fn help_and_version_win_over_everything_after_them() {
        assert_eq!(parse(&["-h", "--bogus"]), Parsed::Help);
        assert_eq!(parse(&["--version"]), Parsed::Version);
    }

    #[test]
    fn bad_input_is_reported_not_ignored() {
        assert_eq!(parse(&["--data-dir"]), Parsed::Invalid("--data-dir 需要一个路径".into()));
        assert_eq!(parse(&["--wat"]), Parsed::Invalid("未知参数: --wat".into()));
    }

    /// H1: 终端初始化失败 (没有 tty 等) 不该被当成「网络错误」报出来——那是 `ClientError::Transport`
    /// 的文案, 和「连不上桌面 app」是两码事, 会把用户带偏。
    #[test]
    fn terminal_failure_message_is_not_reported_as_a_network_error() {
        let err = std::io::Error::other("x");
        let msg = terminal_failure_message(&err, Lang::En);
        assert!(msg.contains("无法初始化终端"), "{msg}");
        assert!(!msg.contains("网络错误"), "{msg}");
    }
}
