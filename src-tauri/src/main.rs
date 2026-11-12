// ============================================================================
//  PureMinecraft —— 极简 MC 启动器（Tauri + Rust）
//
//  目录：<启动器根目录>\.minecraft（相对定位，换盘符/换电脑自动适配）
//  下载走 BMCLAPI（国内镜像），失败自动回退官方源
//  启动：拼 classpath / natives，离线 UUID，拉起 javaw
// ============================================================================

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::{json, Value};
use std::collections::HashMap;
use std::ffi::c_void;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const BMCL: &str = "https://bmclapi2.bangbang93.com";
const MOJANG: &str = "https://launchermeta.mojang.com";
/// 启动器自身的根目录（bin\ 的上一级；不在 bin 下就取 exe 所在目录）
fn base_dir() -> PathBuf {
    let mut p = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    p.pop(); // 去掉 exe 文件名
    if p.file_name().map(|n| n.eq_ignore_ascii_case("bin")).unwrap_or(false) {
        if let Some(parent) = p.parent() {
            return parent.to_path_buf();
        }
    }
    p
}

// ---------------------------------------------------------------- 路径

fn game_root() -> PathBuf {
    base_dir().join(".minecraft")
}
fn versions_dir() -> PathBuf {
    game_root().join("versions")
}
fn libraries_dir() -> PathBuf {
    game_root().join("libraries")
}
fn assets_dir() -> PathBuf {
    game_root().join("assets")
}
fn progress_file() -> PathBuf {
    game_root().join("launcher_progress.json")
}

fn ensure_dirs() {
    for d in [game_root(), versions_dir(), libraries_dir(), assets_dir()] {
        let _ = fs::create_dir_all(d);
    }
}

// ---------------------------------------------------------------- WinHTTP

type HInternet = *mut c_void;

#[link(name = "winhttp")]
extern "system" {
    fn WinHttpOpen(agent: *const u16, access: u32, proxy: *const u16, bypass: *const u16, flags: u32) -> HInternet;
    fn WinHttpConnect(session: HInternet, host: *const u16, port: u16, reserved: u32) -> HInternet;
    fn WinHttpOpenRequest(connect: HInternet, verb: *const u16, path: *const u16,
                          version: *const u16, referrer: *const u16, accept: *const u16, flags: u32) -> HInternet;
    fn WinHttpSendRequest(req: HInternet, headers: *const u16, headers_len: u32,
                          optional: *mut c_void, optional_len: u32, total: u32, ctx: usize) -> i32;
    fn WinHttpReceiveResponse(req: HInternet, reserved: *mut c_void) -> i32;
    fn WinHttpQueryHeaders(req: HInternet, info: u32, name: *const u16,
                           buffer: *mut c_void, len: *mut u32, index: *mut u32) -> i32;
    fn WinHttpQueryDataAvailable(req: HInternet, available: *mut u32) -> i32;
    fn WinHttpReadData(req: HInternet, buffer: *mut c_void, to_read: u32, read: *mut u32) -> i32;
    fn WinHttpCloseHandle(h: HInternet) -> i32;
    fn WinHttpSetTimeouts(h: HInternet, resolve: i32, connect: i32, send: i32, receive: i32) -> i32;
    fn WinHttpSetOption(h: HInternet, option: u32, buffer: *mut c_void, len: u32) -> i32;
}

const Q_STATUS: u32 = 19 | 0x2000_0000;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn split_url(url: &str) -> Option<(String, String, u16, bool)> {
    let (https, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return None;
    };
    let (host_part, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let default_port: u16 = if https { 443 } else { 80 };
    let (host, port) = match host_part.rfind(':') {
        Some(i) => (host_part[..i].to_string(), host_part[i + 1..].parse().unwrap_or(default_port)),
        None => (host_part.to_string(), default_port),
    };
    Some((host, path.to_string(), port, https))
}

unsafe fn open(url: &str) -> Option<(HInternet, HInternet)> {
    let (host, path, port, https) = split_url(url)?;
    let agent = wide("PureMinecraft/1.0");
    let session = WinHttpOpen(agent.as_ptr(), 0, std::ptr::null(), std::ptr::null(), 0);
    if session.is_null() {
        return None;
    }
    WinHttpSetTimeouts(session, 4000, 6000, 6000, 25000);
    // 允许自动跟随 302（Modrinth 的 CDN 跳转）
    let policy: u32 = 0; // WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS
    WinHttpSetOption(session, 76, &policy as *const u32 as *mut c_void, 4);
    let host_w = wide(&host);
    let connect = WinHttpConnect(session, host_w.as_ptr(), port, 0);
    if connect.is_null() {
        WinHttpCloseHandle(session);
        return None;
    }
    let verb = wide("GET");
    let path_w = wide(&path);
    let flags: u32 = if https { 0x0080_0000 } else { 0 };
    let req = WinHttpOpenRequest(connect, verb.as_ptr(), path_w.as_ptr(),
                                 std::ptr::null(), std::ptr::null(), std::ptr::null(), flags);
    if req.is_null() {
        WinHttpCloseHandle(connect);
        WinHttpCloseHandle(session);
        return None;
    }
    Some((req, connect))
}

fn http_text(url: &str) -> Option<String> {
    unsafe {
        let (req, connect) = open(url)?;
        if WinHttpSendRequest(req, std::ptr::null(), 0, std::ptr::null_mut(), 0, 0, 0) == 0
            || WinHttpReceiveResponse(req, std::ptr::null_mut()) == 0
        {
            WinHttpCloseHandle(req);
            WinHttpCloseHandle(connect);
            return None;
        }
        let mut status = [0u32; 1];
        let mut len: u32 = 4;
        let mut idx: u32 = 0;
        WinHttpQueryHeaders(req, Q_STATUS, std::ptr::null(),
                            status.as_mut_ptr() as *mut c_void, &mut len, &mut idx);
        if status[0] != 200 {
            WinHttpCloseHandle(req);
            WinHttpCloseHandle(connect);
            return None;
        }
        let mut out: Vec<u8> = Vec::new();
        let mut buf = vec![0u8; 65536];
        loop {
            let mut avail: u32 = 0;
            if WinHttpQueryDataAvailable(req, &mut avail) == 0 || avail == 0 {
                break;
            }
            let mut read: u32 = 0;
            if WinHttpReadData(req, buf.as_mut_ptr() as *mut c_void, avail.min(buf.len() as u32), &mut read) == 0
                || read == 0
            {
                break;
            }
            out.extend_from_slice(&buf[..read as usize]);
        }
        WinHttpCloseHandle(req);
        WinHttpCloseHandle(connect);
        String::from_utf8(out).ok()
    }
}

/// 下载到文件，返回字节数；失败返回 None
fn http_file(url: &str, dst: &Path) -> Option<u64> {
    unsafe {
        let (req, connect) = open(url)?;
        if WinHttpSendRequest(req, std::ptr::null(), 0, std::ptr::null_mut(), 0, 0, 0) == 0
            || WinHttpReceiveResponse(req, std::ptr::null_mut()) == 0
        {
            WinHttpCloseHandle(req);
            WinHttpCloseHandle(connect);
            return None;
        }
        let mut status = [0u32; 1];
        let mut len: u32 = 4;
        let mut idx: u32 = 0;
        WinHttpQueryHeaders(req, Q_STATUS, std::ptr::null(),
                            status.as_mut_ptr() as *mut c_void, &mut len, &mut idx);
        if status[0] != 200 {
            WinHttpCloseHandle(req);
            WinHttpCloseHandle(connect);
            return None;
        }
        if let Some(parent) = dst.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut file = fs::File::create(dst).ok()?;
        let mut buf = vec![0u8; 262144];
        let mut total: u64 = 0;
        loop {
            let mut avail: u32 = 0;
            if WinHttpQueryDataAvailable(req, &mut avail) == 0 || avail == 0 {
                break;
            }
            let mut read: u32 = 0;
            if WinHttpReadData(req, buf.as_mut_ptr() as *mut c_void, avail.min(buf.len() as u32), &mut read) == 0
                || read == 0
            {
                break;
            }
            if file.write_all(&buf[..read as usize]).is_err() {
                break;
            }
            total += read as u64;
        }
        let _ = file.flush();
        WinHttpCloseHandle(req);
        WinHttpCloseHandle(connect);
        if total > 0 { Some(total) } else { None }
    }
}

/// 先 BMCLAPI 再官方源
fn fetch_first(urls: &[String]) -> Option<String> {
    for u in urls {
        if let Some(t) = http_text(u) {
            return Some(t);
        }
    }
    None
}

fn download_first(urls: &[String], dst: &Path) -> bool {
    for u in urls {
        if http_file(u, dst).is_some() {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------- 进度

#[derive(Default, Clone)]
struct Progress {
    phase: String,
    done: usize,
    total: usize,
    current: String,
    message: String,
    failed: usize,
}

static PROG: Mutex<Progress> = Mutex::new(Progress {
    phase: String::new(),
    done: 0,
    total: 0,
    current: String::new(),
    message: String::new(),
    failed: 0,
});

fn write_progress() {
    let p = PROG.lock().map(|g| g.clone()).unwrap_or_default();
    let v = json!({
        "phase": p.phase, "done": p.done, "total": p.total,
        "current": p.current, "message": p.message, "failed": p.failed
    });
    let path = progress_file();
    let tmp = path.with_extension("tmp");
    if fs::write(&tmp, v.to_string()).is_ok() {
        let _ = fs::rename(&tmp, &path);
    }
}

#[tauri::command]
fn read_progress() -> String {
    fs::read_to_string(progress_file()).unwrap_or_else(|_| r#"{"phase":"idle"}"#.into())
}

// ---------------------------------------------------------------- 版本清单

#[tauri::command]
fn get_manifest() -> Value {
    ensure_dirs();
    let cache = game_root().join("version_manifest_v2.json");
    let text = fetch_first(&[
        format!("{}/mc/game/version_manifest_v2.json", BMCL),
        format!("{}/mc/game/version_manifest_v2.json", MOJANG),
    ]);
    match text {
        Some(t) => {
            let _ = fs::write(&cache, &t);
            serde_json::from_str(&t).unwrap_or(json!({"versions": []}))
        }
        None => {
            if let Ok(t) = fs::read_to_string(&cache) {
                serde_json::from_str(&t).unwrap_or(json!({"versions": []}))
            } else {
                json!({"versions": []})
            }
        }
    }
}

fn local_versions() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(versions_dir()) {
        for e in rd.flatten() {
            if e.path().join(format!("{}.json", e.file_name().to_string_lossy())).exists() {
                out.push(e.file_name().to_string_lossy().to_string());
            }
        }
    }
    out.sort();
    out.reverse();
    out
}

#[tauri::command]
fn list_local() -> Vec<String> {
    local_versions()
}

// ---------------------------------------------------------------- 工具

fn rule_allows(rules: &Value) -> bool {
    // 没有 rules 表示允许；有 rules 时按 windows 平台求值
    let Some(arr) = rules.as_array() else { return true };
    let mut allowed = false;
    for r in arr {
        let action = r["action"].as_str().unwrap_or("allow");
        let os_name = r["os"]["name"].as_str();
        let os_arch = r["os"]["arch"].as_str();
        let mut matches = true;
        if let Some(n) = os_name {
            matches &= n == "windows";
        }
        if let Some(a) = os_arch {
            matches &= a == "x86" || a == "x86_64" || a == std::env::consts::ARCH;
        }
        if matches {
            allowed = action == "allow";
        }
    }
    allowed
}

fn bmcl_maven(url: &str) -> String {
    url.replace("https://libraries.minecraft.net", &format!("{}/maven", BMCL))
        .replace("https://maven.minecraftforge.net", &format!("{}/maven", BMCL))
}

fn bmcl_asset(url: &str) -> String {
    url.replace("https://resources.download.minecraft.net", &format!("{}/assets", BMCL))
}

fn offline_uuid(name: &str) -> String {
    // 与 Java UUID.nameUUIDFromBytes 一致：MD5 后设置版本/变体位
    let digest = md5::compute(format!("OfflinePlayer:{}", name).as_bytes());
    let mut b = digest.0;
    b[6] = (b[6] & 0x0f) | 0x30;
    b[8] = (b[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

// ---------------------------------------------------------------- 安装

struct Job {
    urls: Vec<String>,
    path: PathBuf,
}

fn parallel_download(jobs: Vec<Job>, threads: usize) -> usize {
    let queue = Arc::new(Mutex::new(jobs));
    let failed = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for _ in 0..threads {
        let q = queue.clone();
        let f = failed.clone();
        handles.push(std::thread::spawn(move || loop {
            let job = {
                let mut g = q.lock().unwrap();
                g.pop()
            };
            let Some(job) = job else { break };
            if job.path.exists() {
                if let Ok(mut p) = PROG.lock() {
                    p.done += 1;
                }
                continue;
            }
            let mut ok = false;
            for u in &job.urls {
                if http_file(u, &job.path).is_some() {
                    ok = true;
                    break;
                }
            }
            if !ok {
                f.fetch_add(1, Ordering::SeqCst);
            }
            if let Ok(mut p) = PROG.lock() {
                p.done += 1;
                p.current = job
                    .path
                    .file_name()
                    .map(|x| x.to_string_lossy().to_string())
                    .unwrap_or_default();
                p.failed = f.load(Ordering::SeqCst);
            }
            write_progress();
        }));
    }
    for h in handles {
        let _ = h.join();
    }
    failed.load(Ordering::SeqCst)
}

/// 从缓存的 manifest 里查某个版本的 json 地址
fn manifest_url_for(id: &str) -> Option<String> {
    let cache = game_root().join("version_manifest_v2.json");
    let text = fs::read_to_string(&cache).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    for item in v["versions"].as_array()? {
        if item["id"].as_str() == Some(id) {
            return item["url"].as_str().map(|s| s.to_string());
        }
    }
    None
}

/// 下载并安装一个版本（阻塞，跑在独立线程里）
fn install_version_blocking(id: &str) -> Result<(), String> {
    ensure_dirs();
    *PROG.lock().unwrap() = Progress {
        phase: "meta".into(),
        total: 0,
        done: 0,
        current: String::new(),
        message: "获取版本信息".into(),
        failed: 0,
    };
    write_progress();

    let vdir = versions_dir().join(id);
    fs::create_dir_all(&vdir).map_err(|e| e.to_string())?;
    let vjson_path = vdir.join(format!("{}.json", id));

    // 官方没有 /version/{id}/json，需要从 manifest 里取真实 URL
    let official_url = manifest_url_for(id);
    let mut cands = vec![format!("{}/version/{}/json", BMCL, id)];
    if let Some(u) = official_url {
        cands.push(u);
    }
    let text = fetch_first(&cands)
        .ok_or_else(|| "无法获取版本 JSON（镜像与官方都失败）".to_string())?;

    let v: Value = serde_json::from_str(&text).map_err(|e| format!("版本 JSON 解析失败: {}", e))?;
    fs::write(&vjson_path, &text).map_err(|e| e.to_string())?;

    // client.jar
    let client_url = v["downloads"]["client"]["url"].as_str().unwrap_or("");
    let client_jar = vdir.join(format!("{}.jar", id));
    if !client_jar.exists() {
        *PROG.lock().unwrap() = Progress {
            phase: "client".into(),
            total: 1,
            done: 0,
            current: "client.jar".into(),
            message: String::new(),
            failed: 0,
        };
        write_progress();
        let urls = vec![
            client_url.to_string(),
            format!("{}/version/{}/client", BMCL, id),
        ];
        if !download_first(&urls, &client_jar) {
            return Err("client.jar 下载失败".into());
        }
        PROG.lock().unwrap().done = 1;
    }

    // libraries
    let mut jobs: Vec<Job> = Vec::new();
    if let Some(libs) = v["libraries"].as_array() {
        for lib in libs {
            if !rule_allows(&lib["rules"]) {
                continue;
            }
            if let Some(art) = lib["downloads"]["artifact"].as_object() {
                let path = art.get("path").and_then(|x| x.as_str()).unwrap_or("");
                let url = art.get("url").and_then(|x| x.as_str()).unwrap_or("");
                if path.is_empty() || url.is_empty() {
                    continue;
                }
                jobs.push(Job {
                    urls: vec![url.to_string(), bmcl_maven(url)],
                    path: libraries_dir().join(path),
                });
            }
            // 原生库
            if let Some(natives) = lib["natives"].as_object() {
                if let Some(cls) = natives.get("windows").and_then(|x| x.as_str()) {
                    if let Some(c) = lib["downloads"]["classifiers"][cls].as_object() {
                        let path = c.get("path").and_then(|x| x.as_str()).unwrap_or("");
                        let url = c.get("url").and_then(|x| x.as_str()).unwrap_or("");
                        if !path.is_empty() && !url.is_empty() {
                            jobs.push(Job {
                                urls: vec![url.to_string(), bmcl_maven(url)],
                                path: libraries_dir().join(path),
                            });
                        }
                    }
                }
            }
        }
    }

    // assets
    let mut asset_index = String::new();
    if let Some(ai) = v["assetIndex"].as_object() {
        asset_index = ai.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let url = ai.get("url").and_then(|x| x.as_str()).unwrap_or("");
        let idx_path = assets_dir().join("indexes").join(format!("{}.json", asset_index));
        if !idx_path.exists() && !url.is_empty() {
            let urls = vec![
                format!("{}/assets/{}/indexes/{}.json", BMCL, asset_index, asset_index),
                url.to_string(),
            ];
            download_first(&urls, &idx_path);
        }
        if let Ok(t) = fs::read_to_string(&idx_path) {
            if let Ok(idx) = serde_json::from_str::<Value>(&t) {
                if let Some(objs) = idx["objects"].as_object() {
                    for (_k, o) in objs {
                        let hash = o["hash"].as_str().unwrap_or("");
                        if hash.len() < 2 {
                            continue;
                        }
                        let sub = &hash[..2];
                        let dst = assets_dir().join("objects").join(sub).join(hash);
                        if dst.exists() {
                            continue;
                        }
                        let official = format!(
                            "https://resources.download.minecraft.net/{}/{}",
                            sub, hash
                        );
                        jobs.push(Job {
                            urls: vec![official.clone(), bmcl_asset(&official)],
                            path: dst,
                        });
                    }
                }
            }
        }
    }

    let total = jobs.len();
    {
        let mut p = PROG.lock().unwrap();
        p.phase = "files".into();
        p.total = total;
        p.done = 0;
        p.message = format!("共 {} 个文件", total);
    }
    write_progress();

    let failed = parallel_download(jobs, 24);
    {
        let mut p = PROG.lock().unwrap();
        p.phase = "done".into();
        p.failed = failed;
        p.message = if failed == 0 {
            format!("安装完成：{}", id)
        } else {
            format!("完成，但有 {} 个文件失败（可重试）", failed)
        };
        p.current = String::new();
    }
    write_progress();
    Ok(())
}

#[tauri::command]
fn install_version(id: String) -> String {
    std::thread::spawn(move || {
        let _ = install_version_blocking(&id);
    });
    "started".into()
}

// ---------------------------------------------------------------- Fabric

#[tauri::command]
fn install_fabric(mc: String, loader: String) -> String {
    std::thread::spawn(move || {
        let r = install_fabric_blocking(&mc, &loader);
        let (phase, msg) = match &r {
            Ok(_) => ("done", format!("Fabric 安装完成：{} + loader {}", mc, loader)),
            Err(e) => ("error", format!("Fabric 安装失败：{}", e)),
        };
        {
            let mut p = PROG.lock().unwrap();
            p.phase = phase.to_string();
            p.message = msg;
        }
        write_progress();
    });
    "started".into()
}

fn install_fabric_blocking(mc: &str, loader: &str) -> Result<(), String> {
    ensure_dirs();
    let url = format!("https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json", mc, loader);
    let text = fetch_first(&[url]).ok_or_else(|| "获取 Fabric profile 失败".to_string())?;
    let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;

    let id = v["id"].as_str().unwrap_or(&format!("fabric-loader-{}-{}", loader, mc)).to_string();
    let vdir = versions_dir().join(&id);
    fs::create_dir_all(&vdir).map_err(|e| e.to_string())?;
    fs::write(vdir.join(format!("{}.json", id)), &text).map_err(|e| e.to_string())?;

    // 复制原版 jar 作为 fabric 版本的 jar
    let vanilla = versions_dir().join(mc).join(format!("{}.jar", mc));
    let target = vdir.join(format!("{}.jar", id));
    if vanilla.exists() && !target.exists() {
        let _ = fs::copy(&vanilla, &target);
    }
    // 继承原版 libraries：把原版 json 的 libraries 也下载一遍
    let _ = install_version_blocking(mc);

    let mut jobs = Vec::new();
    if let Some(libs) = v["libraries"].as_array() {
        for lib in libs {
            let name = lib["name"].as_str().unwrap_or("");
            let base = lib["url"].as_str().unwrap_or("https://maven.fabricmc.net/");
            if name.is_empty() {
                continue;
            }
            // group:artifact:version
            let parts: Vec<&str> = name.split(':').collect();
            if parts.len() < 3 {
                continue;
            }
            let path = format!(
                "{}/{}/{}/{}-{}.jar",
                parts[0].replace('.', "/"),
                parts[1],
                parts[2],
                parts[1],
                parts[2]
            );
            let url = format!("{}{}", base, path);
            jobs.push(Job { urls: vec![url], path: libraries_dir().join(path) });
        }
    }
    let total = jobs.len();
    {
        let mut p = PROG.lock().unwrap();
        p.phase = "fabric".into();
        p.total = total;
        p.done = 0;
        p.message = format!("Fabric: {} 个库", total);
    }
    write_progress();
    parallel_download(jobs, 8);
    Ok(())
}

// ---------------------------------------------------------------- 启动

#[tauri::command]
fn launch(id: String, name: String, ram_mb: u32, java_path: String, lang: Option<String>) -> String {
    let lang = lang.unwrap_or_else(|| "zh_cn".to_string());
    match launch_blocking(&id, &name, ram_mb, &java_path, &lang) {
        Ok(cmd) => {
            let mut p = PROG.lock().unwrap();
            p.phase = "launched".into();
            p.message = cmd;
            drop(p);
            write_progress();
            "started".into()
        }
        Err(e) => {
            let mut p = PROG.lock().unwrap();
            p.phase = "error".into();
            p.message = e.clone();
            drop(p);
            write_progress();
            e
        }
    }
}

fn launch_blocking(id: &str, name: &str, ram_mb: u32, java_path: &str, lang: &str) -> Result<String, String> {
    let vdir = versions_dir().join(id);
    let vjson = vdir.join(format!("{}.json", id));
    let text = fs::read_to_string(&vjson).map_err(|_| "找不到版本 JSON".to_string())?;
    let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;

    // Fabric 版本会 inheritFrom，合并父版本
    let mut merged = v.clone();
    if let Some(parent) = v["inheritsFrom"].as_str() {
        let pjson = versions_dir().join(parent).join(format!("{}.json", parent));
        if let Ok(pt) = fs::read_to_string(&pjson) {
            if let Ok(p) = serde_json::from_str::<Value>(&pt) {
                if let Some(parr) = p["libraries"].as_array() {
                    let mut all = parr.clone();
                    if let Some(carr) = merged["libraries"].as_array() {
                        all.extend(carr.clone());
                    }
                    merged["libraries"] = Value::Array(all);
                }
                for key in ["mainClass", "assetIndex", "assets", "downloads", "javaVersion", "arguments"] {
                    if merged[key].is_null() {
                        merged[key] = p[key].clone();
                    }
                }
            }
        }
    }

    // classpath
    let mut cp: Vec<String> = Vec::new();
    if let Some(libs) = merged["libraries"].as_array() {
        for lib in libs {
            if !rule_allows(&lib["rules"]) {
                continue;
            }
            if let Some(art) = lib["downloads"]["artifact"].as_object() {
                if let Some(path) = art.get("path").and_then(|x| x.as_str()) {
                    let full = libraries_dir().join(path.replace('/', "\\"));
                    if full.exists() {
                        cp.push(full.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    // fabric 的库没有 downloads.artifact，按 name 推
    if let Some(libs) = merged["libraries"].as_array() {
        for lib in libs {
            if lib["downloads"]["artifact"].is_object() {
                continue;
            }
            let name_s = lib["name"].as_str().unwrap_or("");
            let parts: Vec<&str> = name_s.split(':').collect();
            if parts.len() < 3 {
                continue;
            }
            let p = format!(
                "{}\\{}\\{}\\{}-{}.jar",
                parts[0].replace('.', "\\"),
                parts[1],
                parts[2],
                parts[1],
                parts[2]
            );
            let full = libraries_dir().join(&p);
            if full.exists() {
                cp.push(full.to_string_lossy().to_string());
            }
        }
    }
    let client_jar = vdir.join(format!("{}.jar", id));
    if client_jar.exists() {
        cp.push(client_jar.to_string_lossy().to_string());
    }
    let cp_str = cp.join(";");

    // natives 解压
    let natives = vdir.join("natives");
    fs::create_dir_all(&natives).map_err(|e| e.to_string())?;
    if let Some(libs) = merged["libraries"].as_array() {
        for lib in libs {
            if let Some(natives_os) = lib["natives"].as_object() {
                if let Some(cls) = natives_os.get("windows").and_then(|x| x.as_str()) {
                    if let Some(c) = lib["downloads"]["classifiers"][cls].as_object() {
                        if let Some(path) = c.get("path").and_then(|x| x.as_str()) {
                            let jar = libraries_dir().join(path.replace('/', "\\"));
                            let _ = extract_zip(&jar, &natives);
                        }
                    }
                }
            }
        }
    }

    let main_class = merged["mainClass"].as_str().unwrap_or("net.minecraft.client.main.Main");
    let asset_index = merged["assetIndex"]["id"]
        .as_str()
        .or_else(|| merged["assets"].as_str())
        .unwrap_or("legacy");

    let uuid = offline_uuid(name);
    let game_dir = game_root();

    let mut args: Vec<String> = vec![
        format!("-Xmx{}M", ram_mb),
        "-Xms512M".into(),
        format!("-Djava.library.path={}", natives.to_string_lossy()),
        format!("-Dminecraft.launcher.brand=PureMinecraft"),
        format!("-Dminecraft.launcher.version=1.0.0"),
        "-cp".into(),
        cp_str,
        main_class.into(),
        "--username".into(), name.into(),
        "--version".into(), id.into(),
        "--gameDir".into(), game_dir.to_string_lossy().to_string(),
        "--assetsDir".into(), assets_dir().to_string_lossy().to_string(),
        "--assetIndex".into(), asset_index.to_string(),
        "--uuid".into(), uuid,
        "--accessToken".into(), "0".into(),
        "--userType".into(), "legacy".into(),
        "--versionType".into(), "release".into(),
    ];

    // arguments.game 里的额外参数
    if let Some(games) = merged["arguments"]["game"].as_array() {
        for a in games {
            if let Some(s) = a.as_str() {
                if s.starts_with("--") && !s.starts_with("--username") && !s.starts_with("--version")
                    && !s.starts_with("--gameDir") && !s.starts_with("--assetsDir")
                    && !s.starts_with("--assetIndex") && !s.starts_with("--uuid")
                    && !s.starts_with("--accessToken") && !s.starts_with("--userType")
                    && !s.starts_with("--versionType")
                {
                    args.push(s.to_string());
                }
            }
        }
    }

    let java = if java_path.trim().is_empty() {
        default_java()
    } else {
        java_path.to_string()
    };
    if !Path::new(&java).exists() {
        return Err(format!("Java 不存在: {}", java));
    }

    // 启动前把游戏语言写进 options.txt
    set_game_language(&game_dir, lang);

    let child = Command::new(&java)
        .args(&args)
        .current_dir(&game_dir)
        .spawn()
        .map_err(|e| format!("启动失败: {}", e))?;

    Ok(format!("已启动 pid={} · {} · {}MB", child.id(), id, ram_mb))
}

/// 修改 options.txt 里的 lang 行（不存在则创建）
fn set_game_language(game_dir: &Path, lang: &str) {
    let path = game_dir.join("options.txt");
    let mut lines: Vec<String> = fs::read_to_string(&path)
        .map(|s| s.lines().map(|x| x.to_string()).collect())
        .unwrap_or_default();
    let mut found = false;
    for l in lines.iter_mut() {
        if l.starts_with("lang:") {
            *l = format!("lang:{}", lang);
            found = true;
        }
    }
    if !found {
        lines.push(format!("lang:{}", lang));
    }
    // options.txt 用 LF 存
    let _ = fs::write(&path, lines.join("\n"));
}

/// 窗口置顶开关
#[tauri::command]
fn set_always_on_top(window: tauri::WebviewWindow, on: bool) -> bool {
    window.set_always_on_top(on).is_ok()
}

/// 按优先级找 Java：runtime\bin → java\*\bin → jre\*\bin → PATH
fn find_java() -> String {
    let base = base_dir();

    let direct = base.join("runtime").join("bin").join("javaw.exe");
    if direct.exists() {
        return direct.to_string_lossy().to_string();
    }

    for folder in ["java", "jre", "runtime"] {
        let dir = base.join(folder);
        if let Ok(rd) = fs::read_dir(&dir) {
            for e in rd.flatten() {
                for exe in ["javaw.exe", "java.exe"] {
                    let c = e.path().join("bin").join(exe);
                    if c.exists() {
                        return c.to_string_lossy().to_string();
                    }
                }
            }
        }
    }

    // 系统 PATH
    for name in ["javaw.exe", "java.exe"] {
        if Command::new("where").arg(name).output().map(|o| o.status.success()).unwrap_or(false) {
            return name.to_string();
        }
    }
    String::new()
}

fn default_java() -> String {
    find_java()
}

/// 自动下载一个 JRE 到 runtime\（Huawei 镜像的 OpenJDK 17）
#[tauri::command]
fn download_java() -> String {
    let base = base_dir();
    let runtime = base.join("runtime");
    let _ = fs::create_dir_all(&runtime);
    let zip = runtime.join("jdk.zip");

    let url = "https://mirrors.huaweicloud.com/openjdk/17.0.2/openjdk-17.0.2_windows-x64_bin.zip";
    {
        let mut p = PROG.lock().unwrap();
        p.phase = "java".into();
        p.total = 1;
        p.done = 0;
        p.message = "下载 Java 运行环境".into();
        p.current = "jdk.zip".into();
    }
    write_progress();

    if !download_first(&[url.to_string()], &zip) {
        let mut p = PROG.lock().unwrap();
        p.phase = "error".into();
        p.message = "Java 下载失败（网络不通）".into();
        drop(p);
        write_progress();
        return "下载失败".into();
    }

    // 解压
    let out = runtime.join("extract");
    let _ = fs::remove_dir_all(&out);
    let _ = fs::create_dir_all(&out);
    let _ = Command::new("powershell")
        .args([
            "-NoProfile", "-Command",
            &format!("Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force", zip.display(), out.display()),
        ])
        .status();

    // 把 jdk-xxx 目录里的内容挪到 runtime\
    let mut moved = false;
    if let Ok(rd) = fs::read_dir(&out) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                let bin = e.path().join("bin").join("javaw.exe");
                if bin.exists() {
                    for item in fs::read_dir(e.path()).into_iter().flatten().flatten() {
                        let dest = runtime.join(item.file_name());
                        let _ = fs::rename(item.path(), &dest);
                    }
                    moved = true;
                    break;
                }
            }
        }
    }
    let _ = fs::remove_file(&zip);
    let _ = fs::remove_dir_all(&out);

    let found = find_java();
    {
        let mut p = PROG.lock().unwrap();
        p.phase = "done".into();
        p.message = if moved && !found.is_empty() {
            format!("Java 就绪：{}", found)
        } else {
            "解压完成但没找到 javaw.exe".into()
        };
        p.done = 1;
    }
    write_progress();
    if moved && !found.is_empty() { found } else { String::new() }
}
fn extract_zip(jar: &Path, dest: &Path) -> Result<(), String> {
    // 极简 zip 读取：调用 PowerShell 解压（避免额外依赖）
    if !jar.exists() {
        return Ok(());
    }
    let _ = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                jar.display(),
                dest.display()
            ),
        ])
        .output();
    Ok(())
}

// ---------------------------------------------------------------- 模组库

/// 搜索 Modrinth 模组。返回原始 JSON 给前端渲染。
#[tauri::command]
fn search_mods(query: String, loader: String, index: u32, sort: String) -> String {
    let q = if query.trim().is_empty() {
        "mod".to_string()
    } else {
        query.trim().to_string()
    };
    let facets = format!(
        "[[%22project_type:mod%22],[%22categories:{}%22]]",
        loader
    );
    let url = format!(
        "https://api.modrinth.com/v2/search?query={}&limit=20&offset={}&index={}&facets={}",
        urlencode(&q), index, sort, facets
    );
    http_text(&url).unwrap_or_else(|| r#"{"hits":[],"total_hits":0,"error":"网络无响应"}"#.into())
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// 安装模组：查版本 → 下载主文件到 .minecraft\mods
/// 模组详情：项目完整信息（简介正文、图集、链接、统计）
#[tauri::command]
fn mod_detail(project_id: String) -> String {
    let url = format!("https://api.modrinth.com/v2/project/{}", project_id);
    http_text(&url).unwrap_or_else(|| r#"{"error":"网络无响应"}"#.into())
}

/// 该模组适用于指定 MC/加载器的版本列表（详情页里选版本装）
#[tauri::command]
fn mod_versions(project_id: String, mc: String, loader: String) -> String {
    let url = format!(
        "https://api.modrinth.com/v2/project/{}/version?game_versions=%5B%22{}%22%5D&loaders=%5B%22{}%22%5D",
        project_id, mc, loader
    );
    http_text(&url).unwrap_or_else(|| "[]".into())
}
#[tauri::command]
fn install_mod(project_id: String, mc: String, loader: String) -> String {
    let url = format!(
        "https://api.modrinth.com/v2/project/{}/version?game_versions=%5B%22{}%22%5D&loaders=%5B%22{}%22%5D",
        project_id, mc, loader
    );
    let Some(text) = http_text(&url) else {
        return "获取版本列表失败".into();
    };
    let Ok(list) = serde_json::from_str::<Value>(&text) else {
        return "版本 JSON 解析失败".into();
    };
    let Some(arr) = list.as_array() else { return "没有可用版本".into() };
    let Some(first) = arr.first() else {
        return format!("没找到适配 {} / {} 的版本", mc, loader);
    };
    let files = &first["files"];
    let Some(farr) = files.as_array() else { return "该版本没有文件".into() };
    let primary = farr
        .iter()
        .find(|f| f["primary"].as_bool().unwrap_or(false))
        .or_else(|| farr.first());
    let Some(file) = primary else { return "没有可下载文件".into() };

    let url = file["url"].as_str().unwrap_or("");
    let name = file["filename"].as_str().unwrap_or("mod.jar");
    let expect = file["size"].as_u64().unwrap_or(0);
    if url.is_empty() {
        return "文件地址为空".into();
    }
    let dir = game_root().join("mods");
    let _ = fs::create_dir_all(&dir);
    let dst = dir.join(name);
    if dst.exists() {
        return format!("已存在：{}", name);
    }
    // 最多试 3 次；下载完必须和官方 size 对上，否则删掉重来
    for attempt in 1..=3 {
        match http_file(url, &dst) {
            Some(sz) => {
                if expect == 0 || sz == expect {
                    return format!("已安装 {}（{} KB）", name, sz / 1024);
                }
                let _ = fs::remove_file(&dst);
                if attempt < 3 {
                    continue;
                }
                return format!(
                    "下载不完整（{}/{} KB），请重试",
                    sz / 1024,
                    expect / 1024
                );
            }
            None => {
                let _ = fs::remove_file(&dst);
                if attempt == 3 {
                    return "下载失败（CDN 无响应，请重试）".into();
                }
            }
        }
    }
    "下载失败".into()
}

/// 已装模组列表
#[tauri::command]
fn list_mods() -> Vec<String> {
    let dir = game_root().join("mods");
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.ends_with(".jar") || n.ends_with(".jar.disabled") {
                out.push(n);
            }
        }
    }
    out.sort();
    out
}
#[tauri::command]
fn tool_status() -> Value {
    let java = default_java();
    // 来自 PATH 的只有裸文件名，视为存在（where 已经确认过）
    let has_path = java.contains('\\') || java.contains('/');
    let exists = if java.is_empty() {
        false
    } else if has_path {
        Path::new(&java).exists()
    } else {
        true
    };
    // 是否来自启动器本地目录（runtime/java/jre），而非 PATH
    let from_local = has_path && java.to_lowercase().contains(&base_dir().to_string_lossy().to_lowercase());
    json!({
        "java": java,
        "java_exists": exists,
        "java_from_local": from_local,
        "root": base_dir().to_string_lossy(),
        "game_dir": game_root().to_string_lossy(),
    })
}

#[tauri::command]
fn open_dir(which: String) -> bool {
    let p = match which.as_str() {
        "game" => game_root(),
        "versions" => versions_dir(),
        _ => base_dir(),
    };
    let _ = fs::create_dir_all(&p);
    Command::new("explorer").arg(p).spawn().is_ok()
}

/// 用系统默认浏览器打开链接
#[tauri::command]
fn open_url(url: String) -> bool {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return false;
    }
    Command::new("cmd").args(["/c", "start", "", &url]).spawn().is_ok()
}

/// 卸载：purge=true 时连 .minecraft（存档/模组）一起删
#[tauri::command]
fn uninstall_app(app: tauri::AppHandle, purge: Option<bool>) {
    let base = base_dir();
    let un = base.join("uninstall.exe");
    if un.exists() {
        let mut cmd = Command::new(&un);
        if purge.unwrap_or(false) {
            cmd.arg("/purge");
        }
        let _ = cmd.spawn();
    }
    app.exit(0);
}

/// 存档体积，卸载前给用户看
#[tauri::command]
fn saves_info() -> serde_json::Value {
    fn dir_size(p: &Path) -> u64 {
        let mut total = 0;
        if let Ok(rd) = fs::read_dir(p) {
            for e in rd.flatten() {
                let path = e.path();
                if path.is_dir() {
                    total += dir_size(&path);
                } else {
                    total += fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                }
            }
        }
        total
    }
    let saves = game_root().join("saves");
    let mut worlds: Vec<String> = Vec::new();
    if let Ok(rd) = fs::read_dir(&saves) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                worlds.push(e.file_name().to_string_lossy().to_string());
            }
        }
    }
    worlds.sort();
    serde_json::json!({
        "path": saves.to_string_lossy(),
        "exists": saves.exists(),
        "size": dir_size(&saves),
        "worlds": worlds,
        "count": worlds.len(),
    })
}

#[tauri::command]
fn read_lang(code: String) -> String {
    let mut root = std::env::current_exe().unwrap_or_default();
    root.pop();
    root.pop();
    fs::read_to_string(root.join("config").join("lang").join(format!("{}.json", code)))
        .unwrap_or_else(|_| "{}".into())
}

fn main() {
    ensure_dirs();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_manifest, list_local, install_version, install_fabric, launch,
            read_progress, tool_status, open_dir, uninstall_app, read_lang, set_always_on_top, search_mods, install_mod, list_mods, download_java, mod_detail, mod_versions, open_url, saves_info
        ])
        .setup(|app| {
            #[cfg(target_os = "windows")]
            {
                use tauri::Manager;
                if let Some(w) = app.get_webview_window("main") {
                    if window_vibrancy::apply_blur(&w, None).is_err() {
                        let _ = window_vibrancy::apply_acrylic(&w, Some((0, 0, 0, 0)));
                    }
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("PureMinecraft 启动失败")
        .run(|_, _| {});
}

#[allow(dead_code)]
fn _unused(_h: HashMap<String, String>) {}
