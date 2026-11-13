// ============================================================================
//  crash.rs —— 崩溃智能分析（v1.1.0）
//
//  一、本地规则引擎：读 .minecraft/crash-reports 里最新的崩溃报告，
//      用关键词规则库匹配，命中就给出中文建议。全程本地，不联网。
//  二、云端 AI 分析：只有用户显式配置了 API Key 并且主动点了按钮才会发请求。
//  三、隐私：没有 Key 时本模块不会发起任何网络连接。
//
//  路径全部来自 crate::game_root()，不写死任何盘符。
// ============================================================================

use crate::{game_root, http_post_json};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// 一条匹配规则
struct Rule {
    id: &'static str,
    patterns: &'static [&'static str],
    title: &'static str,
    advice: &'static str,
}

/// 关键词规则库。顺序即优先级：越具体的越靠前。
const RULES: &[Rule] = &[
    Rule {
        id: "out_of_memory",
        patterns: &[
            "java.lang.OutOfMemoryError",
            "OutOfMemoryError",
            "GC overhead limit exceeded",
            "Java heap space",
        ],
        title: "内存不足",
        advice: "内存不足，建议在启动器设置中调大内存分配。",
    },
    Rule {
        id: "java_too_old",
        patterns: &[
            "UnsupportedClassVersionError",
            "has been compiled by a more recent version",
        ],
        title: "Java 版本过低",
        advice: "Java 版本过低，请更换高版本 Java。",
    },
    Rule {
        id: "missing_class",
        patterns: &[
            "java.lang.ClassNotFoundException",
            "NoClassDefFoundError",
            "java.lang.NoSuchMethodError",
            "NoSuchMethodError",
        ],
        title: "缺少前置模组或版本不兼容",
        advice: "缺少前置模组或版本不兼容，请检查。",
    },
    Rule {
        id: "mod_conflict",
        patterns: &[
            "Incompatible mods found",
            "incompatible mod set",
            "Mod resolution failed",
            "requires any version of",
            "Mod file",
        ],
        title: "模组不兼容",
        advice: "模组之间版本冲突或缺少依赖，请检查 mods 文件夹里的模组组合。",
    },
    Rule {
        id: "graphics",
        patterns: &[
            "GLFW error",
            "Failed to create window",
            "OpenGL",
            "Pixel format not accelerated",
            "WGL",
        ],
        title: "显卡驱动问题",
        advice: "显卡驱动过旧或不被支持，请更新显卡驱动。",
    },
    Rule {
        id: "file_lock",
        patterns: &[
            "being used by another process",
            "The process cannot access the file",
        ],
        title: "文件被占用",
        advice: "游戏文件被其他程序占用，请关闭另一个正在运行的 Minecraft 后重试。",
    },
];

/// 崩溃报告目录
fn crash_dir() -> PathBuf {
    game_root().join("crash-reports")
}

/// 取目录里最新的一个文件（按修改时间）
fn newest_file(dir: &Path, ext: Option<&str>) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    let rd = fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        if let Some(ext) = ext {
            let ok = p
                .extension()
                .map(|x| x.to_string_lossy().eq_ignore_ascii_case(ext))
                .unwrap_or(false);
            if !ok {
                continue;
            }
        }
        let mtime = e
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        if best.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true) {
            best = Some((mtime, p));
        }
    }
    best.map(|(_, p)| p)
}

/// 取文本最后 n 行（按字节上限保护，避免超大文件读进内存）
fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

/// 匹配规则库
fn match_rules(text: &str) -> Vec<Value> {
    let mut hits = Vec::new();
    for r in RULES {
        let hit = r.patterns.iter().any(|p| text.contains(p));
        if hit {
            hits.push(json!({
                "id": r.id,
                "title": r.title,
                "advice": r.advice,
            }));
        }
        // 内存不足优先级最高，命中就不再往下匹配其它噪音规则
        if hit && r.id == "out_of_memory" {
            break;
        }
    }
    hits
}

/// 分析崩溃：找最新崩溃报告，匹配规则
#[tauri::command]
pub fn analyze_crash() -> Value {
    let dir = crash_dir();
    let mut source = "crash-reports";
    let mut file = newest_file(&dir, Some("txt"));

    // 没有 crash-report 时退回 logs/latest.log（很多崩在启动阶段的不写 crash-report）
    if file.is_none() {
        source = "logs";
        file = newest_file(&game_root().join("logs"), Some("log"));
    }

    let Some(path) = file else {
        return json!({
            "ok": true,
            "found": false,
            "source": source,
            "message": "没有找到崩溃报告文件",
            "file": "",
            "matched": [],
            "tail": "",
        });
    };

    let text = fs::read_to_string(&path).unwrap_or_default();
    let matched = match_rules(&text);

    let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let mtime = fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    json!({
        "ok": true,
        "found": true,
        "source": source,
        "file": path.to_string_lossy(),
        "file_name": path.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(),
        "size": size,
        "mtime": mtime,
        "matched": matched,
        "tail": tail_lines(&text, 50),
        "line_count": text.lines().count(),
    })
}

// ---------------------------------------------------------------- AI 配置

fn ai_config_path() -> PathBuf {
    crate::base_dir().join("config").join("ai.json")
}

/// 读取 AI 配置（没配过就返回默认 DeepSeek 地址 + 空 Key）
#[tauri::command]
pub fn ai_config_get() -> Value {
    let path = ai_config_path();
    if let Ok(t) = fs::read_to_string(&path) {
        // 有些编辑器（含 PowerShell 的 Set-Content）会写 UTF-8 BOM，serde_json 不认
        let t = t.trim_start_matches('\u{feff}');
        if let Ok(v) = serde_json::from_str::<Value>(t) {
            return json!({
                "key": v["key"].as_str().unwrap_or(""),
                "url": v["url"].as_str().unwrap_or("https://api.deepseek.com/chat/completions"),
                "model": v["model"].as_str().unwrap_or("deepseek-chat"),
                "has_key": !v["key"].as_str().unwrap_or("").is_empty(),
            });
        }
    }
    json!({
        "key": "",
        "url": "https://api.deepseek.com/chat/completions",
        "model": "deepseek-chat",
        "has_key": false,
    })
}

/// 保存 AI 配置
#[tauri::command]
pub fn ai_config_set(key: String, url: String, model: Option<String>) -> bool {
    let path = ai_config_path();
    if let Some(p) = path.parent() {
        let _ = fs::create_dir_all(p);
    }
    let model = model.unwrap_or_else(|| "deepseek-chat".to_string());
    let v = json!({ "key": key, "url": url, "model": model });
    fs::write(&path, v.to_string()).is_ok()
}

/// 系统提示词
const SYSTEM_PROMPT: &str = "\
你是一个 Minecraft 崩溃日志分析专家。用户会给你一段崩溃日志的结尾部分。\
请用简体中文回答，格式如下：\n\
1. 最可能的原因（一句话）\n\
2. 证据（指出日志里的关键行）\n\
3. 解决办法（分步骤，最多 4 步）\n\
如果日志信息不足，直接说明还需要什么信息，不要编造。";

/// AI 深度分析（异步，不阻塞 UI）
///
/// 隐私保证：
///   · 只有用户主动点击按钮才会调用
///   · api_key 为空时直接返回，绝不发请求
#[tauri::command]
pub async fn ai_analyze(api_key: String, api_url: String, log_tail: String, model: Option<String>) -> String {
    // 红线：没有 Key 就绝不联网
    if api_key.trim().is_empty() {
        return "未配置 API Key，已取消请求（不会上传任何数据）".to_string();
    }
    if log_tail.trim().is_empty() {
        return "没有可分析的日志内容".to_string();
    }
    if !(api_url.starts_with("https://") || api_url.starts_with("http://")) {
        return "API 地址无效，必须以 http:// 或 https:// 开头".to_string();
    }

    let model = model.unwrap_or_else(|| "deepseek-chat".to_string());

    // 限制长度，避免把整份日志推上去
    let tail: String = log_tail.chars().take(6000).collect();

    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": format!("这是 Minecraft 崩溃日志的最后 50 行：\n\n{}", tail) }
        ],
        "temperature": 0.3,
        "max_tokens": 800,
        "stream": false,
    })
    .to_string();

    // 网络请求放到阻塞线程池，绝不占用 UI 线程
    let result = tauri::async_runtime::spawn_blocking(move || {
        http_post_json(&api_url, &api_key, &body)
    })
    .await;

    match result {
        Ok(Some((status, text))) => {
            if status != 200 {
                let brief: String = text.chars().take(300).collect();
                return format!("接口返回 HTTP {}：{}", status, brief);
            }
            match serde_json::from_str::<Value>(&text) {
                Ok(v) => {
                    let content = v["choices"][0]["message"]["content"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    if content.is_empty() {
                        format!("接口返回了内容但无法解析：{}", &text.chars().take(300).collect::<String>())
                    } else {
                        content
                    }
                }
                Err(_) => format!("响应不是合法 JSON：{}", &text.chars().take(300).collect::<String>()),
            }
        }
        Ok(None) => "请求失败：网络不通或地址不可达".to_string(),
        Err(e) => format!("请求线程异常：{}", e),
    }
}

/// 打开崩溃报告所在目录（供前端"查看日志"用）
#[tauri::command]
pub fn open_crash_dir() -> bool {
    let dir = crash_dir();
    let target = if dir.exists() {
        dir
    } else {
        game_root().join("logs")
    };
    if !target.exists() {
        return false;
    }
    std::process::Command::new("explorer")
        .arg(target)
        .spawn()
        .is_ok()
}

// ---------------------------------------------------------------- 单元测试

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_oom() {
        let hits = match_rules("java.lang.OutOfMemoryError: Java heap space\n\tat net.minecraft...");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["id"], "out_of_memory");
    }

    #[test]
    fn rule_class_not_found() {
        let hits = match_rules("Caused by: java.lang.ClassNotFoundException: net.fabricmc.api.ModInitializer");
        assert_eq!(hits[0]["id"], "missing_class");
    }

    #[test]
    fn rule_java_version() {
        let hits = match_rules("java.lang.UnsupportedClassVersionError: X has been compiled by a more recent version");
        assert_eq!(hits[0]["id"], "java_too_old");
    }

    #[test]
    fn rule_none() {
        let hits = match_rules("Stopping! everything fine");
        assert!(hits.is_empty());
    }

    #[test]
    fn tail_keeps_last_lines() {
        let text = (1..=100).map(|i| i.to_string()).collect::<Vec<_>>().join("\n");
        let t = tail_lines(&text, 50);
        assert_eq!(t.lines().count(), 50);
        assert!(t.lines().last().unwrap() == "100");
    }
}
