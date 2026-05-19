use base64::{engine::general_purpose, Engine as _};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use crate::config::{self, VideoFile};
use crate::ffmpeg;

pub const LLAMA_PROPS_URL: &str = "http://127.0.0.1:7080/props";
pub const LLAMA_CHAT_URL: &str = "http://127.0.0.1:7080/v1/chat/completions";

#[derive(Debug, Clone, Default)]
pub struct AiServiceProps {
    pub vision: bool,
    pub audio: bool,
    pub model_alias: String,
}

#[derive(Debug, Clone)]
pub struct TagGroupSchema {
    pub name: String,
    pub max_select: usize,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ScoreRuleSchema {
    pub name: String,
    pub direction: ScoreDirection,
    pub max_delta: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreDirection {
    Add,
    Subtract,
}

#[derive(Debug, Clone)]
pub struct AiTextSchema {
    pub tag_groups: Vec<TagGroupSchema>,
    pub score_rules: Vec<ScoreRuleSchema>,
}

#[derive(Debug, Clone)]
pub struct AiRuntimeConfig {
    pub image_max_pixels: u32,
    pub jpeg_quality: u8,
    pub audio_sample_rate: u32,
    pub audio_clip_seconds: f64,
    pub audio_clips_per_batch: usize,
    pub max_extra_sample_batches: usize,
    pub stream_idle_timeout_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct AiVideoJob {
    pub video: VideoFile,
    pub work_id: u64,
    pub text_settings_json: String,
    pub runtime: AiRuntimeConfig,
    pub allow_audio: bool,
}

#[derive(Debug, Clone)]
pub struct AiAnalysisResult {
    pub labels: Vec<String>,
    pub score: u8,
    pub evidence_summary: String,
}

#[derive(Debug)]
pub enum AiEvent {
    Log { work_id: u64, text: String },
    Preview { work_id: u64, paths: Vec<PathBuf>, times: Vec<f64> },
    Done { work_id: u64, result: AiAnalysisResult },
    Failed { work_id: u64, reason: String },
}

#[derive(Debug)]
enum AiModelDecision {
    Final(AiAnalysisResult),
    NeedMore { reason: String, working_summary: Value },
}

pub fn models_dir() -> PathBuf { config::app_data_dir().join("models") }
pub fn logs_dir() -> PathBuf { config::app_data_dir().join("logs") }

pub fn ensure_models_dir() -> PathBuf {
    let dir = models_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn list_model_scripts() -> Vec<PathBuf> {
    let dir = ensure_models_dir();
    let mut scripts = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() { continue; }
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
            let ok = if cfg!(windows) { ext == "bat" || ext == "cmd" } else { ext == "sh" };
            if ok { scripts.push(path); }
        }
    }
    scripts.sort();
    scripts
}

pub fn probe_llama_props(timeout: Duration) -> Result<AiServiceProps, String> {
    let client = Client::builder().timeout(timeout).build().map_err(|e| e.to_string())?;
    let value: Value = client.get(LLAMA_PROPS_URL).send().map_err(|e| e.to_string())?.json().map_err(|e| e.to_string())?;
    let modalities = value.get("modalities").cloned().unwrap_or(Value::Null);
    let vision = modalities.get("vision").and_then(Value::as_bool).unwrap_or(false);
    let audio = modalities.get("audio").and_then(Value::as_bool).unwrap_or(false);
    let model_alias = value.get("model_alias").or_else(|| value.get("model_path")).and_then(Value::as_str).unwrap_or("local-model").to_string();
    Ok(AiServiceProps { vision, audio, model_alias })
}

pub fn start_model_script(path: &Path) -> Result<Child, String> {
    if !path.exists() { return Err("模型启动脚本不存在".into()); }
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(path);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg(path);
        c
    };
    cmd.current_dir(path.parent().unwrap_or_else(|| Path::new("."))).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().map_err(|e| format!("启动模型脚本失败: {}", e))
}

pub fn stop_process_tree(child: &mut Child) {
    let pid = child.id();
    if cfg!(windows) {
        let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    } else {
        let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null()).status();
        std::thread::sleep(Duration::from_millis(500));
        let _ = Command::new("kill").args(["-KILL", &pid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub fn default_ai_text_settings() -> String {
    r#"{
  "tag_groups": {
    "视频内容": {
      "max_select": 3,
      "tags": ["风景", "人文", "美食"]
    },
    "视频画质": {
      "max_select": 1,
      "tags": ["老电影", "清晰", "模糊"]
    },
    "声音类型": {
      "max_select": 2,
      "tags": ["无人声", "人声", "音乐", "嘈杂"]
    }
  },
  "score_rules": [
    {
      "name": "画面清晰",
      "direction": "add",
      "max_delta": 15
    },
    {
      "name": "声音细节丰富",
      "direction": "add",
      "max_delta": 10
    },
    {
      "name": "声音嘈杂",
      "direction": "subtract",
      "max_delta": 20
    }
  ]
}"#.to_string()
}

pub fn validate_ai_text_settings(text: &str) -> Result<AiTextSchema, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| format!("AI 文本设置 JSON 无效: {}", e))?;
    let groups_value = root.get("tag_groups").and_then(Value::as_object).ok_or_else(|| "AI 文本设置缺少 tag_groups 对象".to_string())?;
    if groups_value.is_empty() { return Err("tag_groups 不能为空".into()); }
    let mut tag_groups = Vec::new();
    for (group_name, group_value) in groups_value.iter() {
        let max_select = group_value.get("max_select").and_then(Value::as_u64).ok_or_else(|| format!("tag_groups.{}.max_select 必须是非负整数", group_name))? as usize;
        let tags_arr = group_value.get("tags").and_then(Value::as_array).ok_or_else(|| format!("tag_groups.{}.tags 必须是字符串数组", group_name))?;
        if tags_arr.is_empty() { return Err(format!("tag_groups.{}.tags 不能为空", group_name)); }
        let mut tags = Vec::new();
        let mut seen = HashSet::new();
        for item in tags_arr {
            let tag = item.as_str().ok_or_else(|| format!("tag_groups.{}.tags 中存在非字符串标签", group_name))?.trim().to_string();
            if tag.is_empty() { return Err(format!("tag_groups.{}.tags 中存在空标签", group_name)); }
            if seen.insert(tag.clone()) { tags.push(tag); }
        }
        tag_groups.push(TagGroupSchema { name: group_name.clone(), max_select, tags });
    }
    let mut score_rules = Vec::new();
    if let Some(rules) = root.get("score_rules") {
        let arr = rules.as_array().ok_or_else(|| "score_rules 必须是数组".to_string())?;
        for rule in arr {
            let name = rule.get("name").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| "score_rules[].name 必须是非空字符串".to_string())?.to_string();
            let direction = match rule.get("direction").and_then(Value::as_str).unwrap_or("") {
                "add" => ScoreDirection::Add,
                "subtract" => ScoreDirection::Subtract,
                other => return Err(format!("score_rules.{} direction 无效: {}", name, other)),
            };
            let max_delta = rule.get("max_delta").and_then(Value::as_i64).ok_or_else(|| format!("score_rules.{}.max_delta 必须是整数", name))? as i32;
            if max_delta < 0 || max_delta > 100 { return Err(format!("score_rules.{}.max_delta 必须在 0-100", name)); }
            score_rules.push(ScoreRuleSchema { name, direction, max_delta });
        }
    }
    Ok(AiTextSchema { tag_groups, score_rules })
}

pub fn spawn_video_analysis(job: AiVideoJob, tx: mpsc::Sender<AiEvent>) {
    std::thread::spawn(move || {
        let result = run_video_analysis(job.clone(), tx.clone());
        if let Err(reason) = result { let _ = tx.send(AiEvent::Failed { work_id: job.work_id, reason }); }
    });
}

fn send_log(tx: &mpsc::Sender<AiEvent>, work_id: u64, text: impl Into<String>) {
    let _ = tx.send(AiEvent::Log { work_id, text: text.into() });
}

fn compact_for_log(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in text.chars().enumerate() {
        if i >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

fn path_name(path: &Path) -> String {
    path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string())
}

fn run_video_analysis(job: AiVideoJob, tx: mpsc::Sender<AiEvent>) -> Result<(), String> {
    let schema = validate_ai_text_settings(&job.text_settings_json)?;
    let mut video = job.video.clone();
    let duration = video.ensure_duration().max(0.1);
    let mut working_summary = Value::Null;
    send_log(&tx, job.work_id, format!("AI：开始分析 {}。", job.video.filename));
    send_log(&tx, job.work_id, format!("AI：视频长度 {:.1} 秒。", duration));
    let temp_dir = tempfile::Builder::new().prefix("video_tagger_ai_").tempdir().map_err(|e| e.to_string())?;
    let max_batches = job.runtime.max_extra_sample_batches + 1;
    for batch_index in 0..max_batches {
        let is_last_budget = batch_index + 1 >= max_batches;
        let times = sample_times(duration, batch_index);
        send_log(&tx, job.work_id, if batch_index == 0 { "AI：正在进行全片均匀采样。".to_string() } else { format!("AI：正在获取第 {} / {} 次追加采样。", batch_index, job.runtime.max_extra_sample_batches) });
        send_log(&tx, job.work_id, format!("程序 -> AI：本轮采样时间点：{}。", times.iter().map(|t| format!("{:.1}s", t)).collect::<Vec<_>>().join(" / ")));
        let image_paths = build_ai_images(&job, temp_dir.path(), &times)?;
        let _ = tx.send(AiEvent::Preview { work_id: job.work_id, paths: image_paths.clone(), times: times.clone() });
        let audio_paths = if job.allow_audio && job.runtime.audio_clips_per_batch > 0 { build_ai_audio(&job, temp_dir.path(), &times)? } else { Vec::new() };
        send_log(&tx, job.work_id, format!("AI：已准备 {} 张图片、{} 段音频。", image_paths.len(), audio_paths.len()));
        let decision = call_with_retry(&job, &schema, &image_paths, &audio_paths, &working_summary, is_last_budget, &tx)?;
        match decision {
            AiModelDecision::Final(result) => {
                send_log(&tx, job.work_id, format!("AI：最终标签：{}。", if result.labels.is_empty() { "无".to_string() } else { result.labels.join("、") }));
                send_log(&tx, job.work_id, format!("AI：评分：{} 分。", result.score));
                if !result.evidence_summary.trim().is_empty() { send_log(&tx, job.work_id, format!("AI：依据：{}", result.evidence_summary)); }
                let _ = tx.send(AiEvent::Done { work_id: job.work_id, result });
                return Ok(());
            }
            AiModelDecision::NeedMore { reason, working_summary: next_summary } => {
                working_summary = next_summary;
                if is_last_budget {
                    send_log(&tx, job.work_id, "AI：追加采样次数已达到上限，将基于现有证据生成最终标签。".to_string());
                    let decision = call_with_retry(&job, &schema, &[], &[], &working_summary, true, &tx)?;
                    if let AiModelDecision::Final(result) = decision {
                        send_log(&tx, job.work_id, format!("AI：最终标签：{}。", if result.labels.is_empty() { "无".to_string() } else { result.labels.join("、") }));
                        send_log(&tx, job.work_id, format!("AI：评分：{} 分。", result.score));
                        let _ = tx.send(AiEvent::Done { work_id: job.work_id, result });
                        return Ok(());
                    }
                    return Err("达到追加采样上限后，AI 仍未输出 final".into());
                }
                send_log(&tx, job.work_id, format!("AI：当前信息不足，正在获取下一组截图和音频。原因：{}", reason));
            }
        }
    }
    Err("AI 未能生成最终标签".into())
}

fn build_ai_images(job: &AiVideoJob, temp_dir: &Path, times: &[f64]) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    for (idx, time) in times.iter().enumerate() {
        let path = temp_dir.join(format!("ai_{}_img_{:02}.jpg", job.work_id, idx));
        ffmpeg::extract_ai_image(&job.video.path, *time, job.runtime.image_max_pixels, job.runtime.jpeg_quality, &path)?;
        paths.push(path);
    }
    Ok(paths)
}

fn build_ai_audio(job: &AiVideoJob, temp_dir: &Path, times: &[f64]) -> Result<Vec<PathBuf>, String> {
    let count = job.runtime.audio_clips_per_batch.min(times.len());
    if count == 0 { return Ok(Vec::new()); }
    let indices = evenly_spaced_indices(times.len(), count);
    let mut paths = Vec::new();
    for (audio_idx, time_idx) in indices.iter().enumerate() {
        let center = times[*time_idx];
        let start = (center - job.runtime.audio_clip_seconds / 2.0).max(0.0);
        let path = temp_dir.join(format!("ai_{}_aud_{:02}.wav", job.work_id, audio_idx));
        ffmpeg::extract_ai_audio_clip(&job.video.path, start, job.runtime.audio_clip_seconds, job.runtime.audio_sample_rate, &path)?;
        paths.push(path);
    }
    Ok(paths)
}

fn sample_times(duration: f64, batch_index: usize) -> Vec<f64> {
    let duration = duration.max(0.1);
    let (start_ratio, end_ratio) = match batch_index { 0 => (0.0, 1.0), 1 => (0.0, 0.25), 2 => (0.375, 0.625), 3 => (0.75, 1.0), _ => (0.25, 0.75) };
    let start = duration * start_ratio;
    let end = duration * end_ratio;
    let count = 10usize;
    (0..count).map(|i| { let t = start + (end - start) * (i as f64) / ((count - 1) as f64); t.clamp(0.0, (duration - 0.35).max(0.0)) }).collect()
}

fn evenly_spaced_indices(total: usize, count: usize) -> Vec<usize> {
    if total == 0 || count == 0 { return Vec::new(); }
    if count == 1 { return vec![total / 2]; }
    (0..count).map(|i| ((i as f64) * ((total - 1) as f64) / ((count - 1) as f64)).round() as usize).collect()
}

fn call_with_retry(job: &AiVideoJob, schema: &AiTextSchema, image_paths: &[PathBuf], audio_paths: &[PathBuf], working_summary: &Value, force_final: bool, tx: &mpsc::Sender<AiEvent>) -> Result<AiModelDecision, String> {
    let mut last_error = String::new();
    for attempt in 0..=1 {
        if attempt == 1 { send_log(tx, job.work_id, format!("AI：输出格式不符合要求，正在自动重试。错误：{}", last_error)); }
        let prompt = build_prompt(&job.video, schema, working_summary, force_final, attempt == 1, &last_error);
        save_raw_log("ai_last_prompt.txt", &prompt);
        send_log(tx, job.work_id, format!("程序 -> AI：发送提示词，force_final={}，重试={}。完整提示词已保存到 logs/ai_last_prompt.txt。", force_final, attempt == 1));
        send_log(tx, job.work_id, format!("程序 -> AI：提示词预览：{}", compact_for_log(&prompt.replace('\n', " "), 420)));
        if !image_paths.is_empty() {
            send_log(tx, job.work_id, format!("程序 -> AI：附加图片 {} 张：{}", image_paths.len(), image_paths.iter().map(|p| path_name(p)).collect::<Vec<_>>().join("、")));
        }
        if !audio_paths.is_empty() {
            send_log(tx, job.work_id, format!("程序 -> AI：附加音频 {} 段：{}", audio_paths.len(), audio_paths.iter().map(|p| path_name(p)).collect::<Vec<_>>().join("、")));
        }
        let raw = call_llama(&prompt, image_paths, audio_paths, job.runtime.stream_idle_timeout_seconds, tx, job.work_id)?;
        send_log(tx, job.work_id, "AI：已收到模型输出，正在校验格式。".to_string());
        save_raw_log("ai_last_raw_response.txt", &raw);
        send_log(tx, job.work_id, "程序：完整模型原文已保存到 logs/ai_last_raw_response.txt。".to_string());
        match parse_model_decision(&raw, schema, force_final) {
            Ok(decision) => {
                match &decision {
                    AiModelDecision::Final(result) => {
                        send_log(tx, job.work_id, format!("程序：解析到 final。标签={}，评分={}。", if result.labels.is_empty() { "无".to_string() } else { result.labels.join("、") }, result.score));
                    }
                    AiModelDecision::NeedMore { reason, .. } => {
                        send_log(tx, job.work_id, format!("程序：解析到 need_more_samples。原因：{}", reason));
                    }
                }
                return Ok(decision);
            }
            Err(err) => {
                send_log(tx, job.work_id, format!("程序：模型输出校验失败：{}", err));
                last_error = err;
            }
        }
    }
    Err(last_error)
}

fn build_prompt(video: &VideoFile, schema: &AiTextSchema, working_summary: &Value, force_final: bool, retry: bool, last_error: &str) -> String {
    let tag_groups_text: Vec<Value> = schema.tag_groups.iter().map(|g| json!({ "name": g.name, "max_select": g.max_select, "tags": g.tags })).collect();
    let score_rules_text: Vec<Value> = schema.score_rules.iter().map(|r| json!({ "name": r.name, "direction": match r.direction { ScoreDirection::Add => "add", ScoreDirection::Subtract => "subtract" }, "max_delta": r.max_delta })).collect();
    format!(r#"你是 video-tagger 的本地 AI 视频分析器。你只能根据用户提供的图片、音频、视频长度、上一轮摘要、标签库、评分规则做判断。
硬性规则：
1. 标签只能从 tag_groups 中选择，不能创造新标签。
2. 每个标签栏最多选择 max_select 个标签，可以空选。
3. 标签输出必须按 tag_groups 的栏位名称组织。
4. 评分基础分固定 50，只能使用 score_rules 中列出的评分项。
5. 评分 delta 必须是整数。add 项范围 0 到 max_delta；subtract 项范围 -max_delta 到 0。
6. final 分数范围 0 到 100。
7. 输出必须是一个 JSON 对象，不要输出 Markdown，不要输出解释性正文。
8. 如果证据不足，可以输出 status=need_more_samples；但如果 force_final=true，必须输出 status=final。
当前视频：{video_name}
视频长度：{duration:.1} 秒
force_final：{force_final}
{retry_text}
标签库 tag_groups：
{tag_groups}
评分规则 score_rules：
{score_rules}
上一轮 working_summary：
{working_summary}
final JSON 格式：
{{
  "status": "final",
  "tags": {{ "标签栏名称": ["标签"] }},
  "score": {{ "base": 50, "details": [{{ "rule": "评分项名称", "delta": 0, "reason": "为什么给这个分" }}], "final": 50 }},
  "evidence": {{ "summary": "总体证据摘要", "tags": {{ "标签栏名称": "选择这些标签的依据" }} }},
  "working_summary": {{ "visual": "已观察到的画面摘要", "audio": "已观察到的音频摘要", "uncertainties": [] }}
}}
need_more_samples JSON 格式：
{{
  "status": "need_more_samples",
  "reason": "为什么当前证据不足",
  "working_summary": {{ "visual": "已观察到的画面摘要", "audio": "已观察到的音频摘要", "candidate_tags": {{ "标签栏名称": ["候选标签"] }}, "uncertainties": [] }}
}}
"#, video_name = video.filename, duration = video.duration_secs.unwrap_or(0.0), force_final = force_final, retry_text = if retry { format!("上次输出错误：{}。这次必须修正格式。", last_error) } else { String::new() }, tag_groups = serde_json::to_string_pretty(&tag_groups_text).unwrap_or_default(), score_rules = serde_json::to_string_pretty(&score_rules_text).unwrap_or_default(), working_summary = if working_summary.is_null() { "null".to_string() } else { serde_json::to_string_pretty(working_summary).unwrap_or_default() })
}

fn call_llama(prompt: &str, image_paths: &[PathBuf], audio_paths: &[PathBuf], idle_timeout_seconds: u64, tx: &mpsc::Sender<AiEvent>, work_id: u64) -> Result<String, String> {
    let client = Client::builder().timeout(Duration::from_secs(idle_timeout_seconds.max(1))).connect_timeout(Duration::from_secs(10)).build().map_err(|e| e.to_string())?;
    let mut content = Vec::new();
    content.push(json!({ "type": "text", "text": prompt }));
    for path in image_paths {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let b64 = general_purpose::STANDARD.encode(data);
        content.push(json!({ "type": "image_url", "image_url": { "url": format!("data:image/jpeg;base64,{}", b64) } }));
    }
    for path in audio_paths {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let b64 = general_purpose::STANDARD.encode(data);
        content.push(json!({ "type": "input_audio", "input_audio": { "data": b64, "format": "wav" } }));
    }
    let body = json!({ "model": "local-model", "messages": [{ "role": "user", "content": content }], "temperature": 0.1, "max_tokens": 1200, "stream": true });
    send_log(tx, work_id, format!("程序 -> AI：POST {}，stream=true，content 项={}。", LLAMA_CHAT_URL, 1 + image_paths.len() + audio_paths.len()));
    let response = client.post(LLAMA_CHAT_URL).json(&body).send().map_err(|e| format!("AI 请求失败: {}", e))?;
    if !response.status().is_success() { return Err(format!("AI 服务返回错误状态: {}", response.status())); }
    send_log(tx, work_id, "AI -> 程序：开始接收流式输出。".to_string());
    let reader = BufReader::new(response);
    let mut out = String::new();
    let mut pending = String::new();
    for line in reader.lines() {
        let line = line.map_err(|e| format!("AI 流式读取失败或超时: {}", e))?;
        let line = line.trim();
        if line.is_empty() || !line.starts_with("data:") { continue; }
        let data = line.trim_start_matches("data:").trim();
        if data == "[DONE]" { break; }
        let value: Value = match serde_json::from_str(data) { Ok(v) => v, Err(_) => continue };
        let delta = value.pointer("/choices/0/delta/content").and_then(Value::as_str)
            .or_else(|| value.pointer("/choices/0/message/content").and_then(Value::as_str));
        if let Some(s) = delta {
            out.push_str(s);
            pending.push_str(s);
            if pending.chars().count() >= 160 || pending.contains('\n') {
                send_log(tx, work_id, format!("AI -> 程序：{}", pending.trim()));
                pending.clear();
            }
        }
    }
    if !pending.trim().is_empty() {
        send_log(tx, work_id, format!("AI -> 程序：{}", pending.trim()));
    }
    if out.trim().is_empty() { return Err("AI 返回内容为空".into()); }
    Ok(out)
}

fn parse_model_decision(raw: &str, schema: &AiTextSchema, force_final: bool) -> Result<AiModelDecision, String> {
    let json_text = extract_json_object(raw).ok_or_else(|| "模型输出中找不到 JSON 对象".to_string())?;
    let value: Value = serde_json::from_str(json_text).map_err(|e| format!("模型输出不是合法 JSON: {}", e))?;
    let status = value.get("status").and_then(Value::as_str).unwrap_or("");
    match status {
        "final" => parse_final_result(&value, schema).map(AiModelDecision::Final),
        "need_more_samples" if !force_final => { let reason = value.get("reason").and_then(Value::as_str).unwrap_or("证据不足").to_string(); let working_summary = value.get("working_summary").cloned().unwrap_or(Value::Null); Ok(AiModelDecision::NeedMore { reason, working_summary }) }
        "need_more_samples" => Err("force_final=true 时不能请求更多样本".into()),
        other => Err(format!("未知 status: {}", other)),
    }
}

fn parse_final_result(value: &Value, schema: &AiTextSchema) -> Result<AiAnalysisResult, String> {
    let tags_obj = value.get("tags").and_then(Value::as_object).ok_or_else(|| "final.tags 必须是对象".to_string())?;
    let mut flat_labels = Vec::new();
    let mut used = HashSet::new();
    for group in &schema.tag_groups {
        let arr = tags_obj.get(&group.name).and_then(Value::as_array).cloned().unwrap_or_default();
        if arr.len() > group.max_select { return Err(format!("标签栏 {} 选择数量超过 max_select", group.name)); }
        let valid: HashSet<&str> = group.tags.iter().map(|s| s.as_str()).collect();
        for item in arr {
            let tag = item.as_str().ok_or_else(|| format!("标签栏 {} 中存在非字符串标签", group.name))?.to_string();
            if !valid.contains(tag.as_str()) { return Err(format!("标签 {} 不属于标签栏 {}", tag, group.name)); }
            if used.insert(tag.clone()) { flat_labels.push(tag); }
        }
    }
    let score = value.get("score").ok_or_else(|| "final.score 缺失".to_string())?;
    let details = score.get("details").and_then(Value::as_array).ok_or_else(|| "score.details 必须是数组".to_string())?;
    let mut rule_map: HashMap<&str, &ScoreRuleSchema> = HashMap::new();
    for rule in &schema.score_rules { rule_map.insert(rule.name.as_str(), rule); }
    let mut sum_delta = 0i32;
    for detail in details {
        let rule_name = detail.get("rule").and_then(Value::as_str).ok_or_else(|| "score.details[].rule 必须是字符串".to_string())?;
        let delta = detail.get("delta").and_then(Value::as_i64).ok_or_else(|| format!("评分项 {} 的 delta 必须是整数", rule_name))? as i32;
        let Some(rule) = rule_map.get(rule_name) else { return Err(format!("评分项 {} 不在 score_rules 中", rule_name)); };
        let clamped = match rule.direction { ScoreDirection::Add => delta.clamp(0, rule.max_delta), ScoreDirection::Subtract => delta.clamp(-rule.max_delta, 0) };
        sum_delta += clamped;
    }
    let final_score = (50 + sum_delta).clamp(0, 100) as u8;
    let evidence_summary = value.pointer("/evidence/summary").and_then(Value::as_str).unwrap_or("").to_string();
    Ok(AiAnalysisResult { labels: flat_labels, score: final_score, evidence_summary })
}

fn extract_json_object(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end <= start { return None; }
    raw.get(start..=end)
}

pub fn point_label(score: u8) -> String { format!("point{:03}", score.min(100)) }

pub fn save_raw_log(name: &str, content: &str) {
    let dir = logs_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(name), content);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiFailureRecord {
    pub filename: String,
    pub reason: String,
}