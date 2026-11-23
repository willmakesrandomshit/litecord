#![forbid(unsafe_code)]

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use boa_engine::object::FunctionObjectBuilder;
use boa_engine::{Context, JsString, JsValue, NativeFunction, Source};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_PLUGIN_LOGS: usize = 200;
const JS_LOOP_ITERATION_LIMIT: u64 = 50_000;
const JS_RECURSION_LIMIT: usize = 64;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("Plugin '{0}' not found")]
    NotFound(String),
    #[error("Plugin '{id}' is blocked: {reason}")]
    Blocked { id: String, reason: String },
    #[error("JavaScript execution error: {0}")]
    Js(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginPermission {
    MessageHooks,
    SettingsReadWrite,
    Logging,
}

impl PluginPermission {
    pub fn label(self) -> &'static str {
        match self {
            Self::MessageHooks => "Message Hooks",
            Self::SettingsReadWrite => "Plugin Settings",
            Self::Logging => "Sandboxed Logs",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatibilityStatus {
    Native,
    VencordUnchanged,
    VencordAdapter,
    RequiresDiscordInternals,
    Unsupported,
}

impl CompatibilityStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Native => "Native Plugin",
            Self::VencordUnchanged => "Vencord (Unchanged)",
            Self::VencordAdapter => "Vencord (Adapter)",
            Self::RequiresDiscordInternals => "Blocked (Webpack/Internals)",
            Self::Unsupported => "Unsupported",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SettingType {
    Boolean,
    String,
    Integer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SettingValue {
    Boolean(bool),
    String(String),
    Integer(i64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingDef {
    pub key: String,
    pub description: String,
    pub kind: SettingType,
    pub default: SettingValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub permissions: Vec<PluginPermission>,
    pub compatibility: CompatibilityStatus,
    pub compatibility_note: String,
    pub settings_schema: Vec<SettingDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginState {
    Disabled,
    Active,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct RegisteredPlugin {
    pub manifest: PluginManifest,
    pub source: String,
    pub state: PluginState,
    pub settings: HashMap<String, SettingValue>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedPluginConfig {
    pub enabled_ids: Vec<String>,
    pub settings_by_plugin: HashMap<String, HashMap<String, SettingValue>>,
}

struct JsSandbox {
    context: Context,
}

impl JsSandbox {
    fn new() -> Self {
        let mut context = Context::default();
        context
            .runtime_limits_mut()
            .set_loop_iteration_limit(JS_LOOP_ITERATION_LIMIT);
        context
            .runtime_limits_mut()
            .set_recursion_limit(JS_RECURSION_LIMIT);

        let log_fn = FunctionObjectBuilder::new(
            context.realm(),
            NativeFunction::from_fn_ptr(|_this, args, ctx| {
                let msg = args
                    .first()
                    .map(|v| v.to_string(ctx))
                    .transpose()?
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                tracing::debug!(target: "zero_plugin_js", "{msg}");
                Ok(JsValue::undefined())
            }),
        )
        .name(JsString::from("__zero_log"))
        .length(1)
        .build();

        let _ = context.register_global_property(
            JsString::from("__zero_log"),
            log_fn,
            boa_engine::property::Attribute::READONLY,
        );

        Self { context }
    }

    fn eval(&mut self, code: &str) -> Result<JsValue, PluginError> {
        self.context
            .eval(Source::from_bytes(code.as_bytes()))
            .map_err(|e| PluginError::Js(e.to_string()))
    }
}

pub struct PluginManager {
    plugins: Vec<RegisteredPlugin>,
    sandbox: Option<JsSandbox>,
    prelude_js: String,
    logs: VecDeque<String>,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            sandbox: None,
            prelude_js: String::new(),
            logs: VecDeque::with_capacity(MAX_PLUGIN_LOGS),
        }
    }

    pub fn set_prelude(&mut self, prelude_js: impl Into<String>) {
        self.prelude_js = prelude_js.into();
    }

    pub fn plugins(&self) -> &[RegisteredPlugin] {
        &self.plugins
    }

    pub fn is_sandbox_loaded(&self) -> bool {
        self.sandbox.is_some()
    }

    pub fn active_count(&self) -> usize {
        self.plugins
            .iter()
            .filter(|p| p.state == PluginState::Active)
            .count()
    }

    pub fn logs(&self) -> &VecDeque<String> {
        &self.logs
    }

    fn push_log(&mut self, msg: impl Into<String>) {
        if self.logs.len() >= MAX_PLUGIN_LOGS {
            self.logs.pop_front();
        }
        self.logs.push_back(msg.into());
    }

    pub fn register_plugin(&mut self, manifest: PluginManifest, source: impl Into<String>) {
        let mut settings = HashMap::new();
        for def in &manifest.settings_schema {
            settings.insert(def.key.clone(), def.default.clone());
        }
        if let Some(existing) = self
            .plugins
            .iter_mut()
            .find(|p| p.manifest.id == manifest.id)
        {
            existing.manifest = manifest;
            existing.source = source.into();
            existing.state = PluginState::Disabled;
            existing.settings = settings;
        } else {
            self.plugins.push(RegisteredPlugin {
                manifest,
                source: source.into(),
                state: PluginState::Disabled,
                settings,
            });
        }
    }

    fn ensure_sandbox(&mut self) -> Result<&mut JsSandbox, PluginError> {
        if self.sandbox.is_none() {
            let mut sb = JsSandbox::new();
            if !self.prelude_js.is_empty() {
                sb.eval(&self.prelude_js)?;
            }
            sb.eval("var __zero_plugins = {};")?;
            self.sandbox = Some(sb);
        }
        Ok(self.sandbox.as_mut().expect("sandbox initialized"))
    }

    pub fn enable_plugin(&mut self, id: &str) -> Result<(), PluginError> {
        let idx = self
            .plugins
            .iter()
            .position(|p| p.manifest.id == id)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))?;

        let compat = self.plugins[idx].manifest.compatibility;
        if matches!(
            compat,
            CompatibilityStatus::RequiresDiscordInternals | CompatibilityStatus::Unsupported
        ) {
            let reason = self.plugins[idx].manifest.compatibility_note.clone();
            self.plugins[idx].state = PluginState::Failed(reason.clone());
            return Err(PluginError::Blocked {
                id: id.to_string(),
                reason,
            });
        }

        let source = self.plugins[idx].source.clone();
        let settings_json = serde_json::to_string(&self.plugins[idx].settings)?;
        let safe_id = serde_json::to_string(id)?;

        let init_result = {
            let sb = self.ensure_sandbox()?;
            let script = format!(
                "(function() {{\n\
                    var __plugin_id = {safe_id};\n\
                    var __settings = {settings_json};\n\
                    {source}\n\
                    if (typeof __zero_last_plugin !== 'undefined' && __zero_last_plugin) {{\n\
                        __zero_plugins[__plugin_id] = __zero_last_plugin;\n\
                        if (__zero_plugins[__plugin_id].__applySettings) {{\n\
                            __zero_plugins[__plugin_id].__applySettings(__settings);\n\
                        }}\n\
                        if (typeof __zero_plugins[__plugin_id].start === 'function') {{\n\
                            __zero_plugins[__plugin_id].start();\n\
                        }}\n\
                    }}\n\
                }})();"
            );
            sb.eval(&script)
        };

        match init_result {
            Ok(_) => {
                self.plugins[idx].state = PluginState::Active;
                self.push_log(format!("Enabled plugin '{id}'"));
                Ok(())
            }
            Err(e) => {
                let err_msg = e.to_string();
                self.plugins[idx].state = PluginState::Failed(err_msg.clone());
                self.push_log(format!("Plugin '{id}' failed to start: {err_msg}"));
                Err(e)
            }
        }
    }

    pub fn disable_plugin(&mut self, id: &str) -> Result<(), PluginError> {
        let idx = self
            .plugins
            .iter()
            .position(|p| p.manifest.id == id)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))?;

        if let Some(sb) = &mut self.sandbox {
            let safe_id = serde_json::to_string(id)?;
            let stop_script = format!(
                "(function() {{\n\
                    var p = __zero_plugins[{safe_id}];\n\
                    if (p && typeof p.stop === 'function') {{ p.stop(); }}\n\
                    delete __zero_plugins[{safe_id}];\n\
                }})();"
            );
            let _ = sb.eval(&stop_script);
        }

        self.plugins[idx].state = PluginState::Disabled;
        self.push_log(format!("Disabled plugin '{id}'"));

        if self.active_count() == 0 {
            self.sandbox = None;
        }
        Ok(())
    }

    pub fn set_plugin_setting(
        &mut self,
        id: &str,
        key: &str,
        value: SettingValue,
    ) -> Result<(), PluginError> {
        let idx = self
            .plugins
            .iter()
            .position(|p| p.manifest.id == id)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))?;
        self.plugins[idx].settings.insert(key.to_string(), value);

        if self.plugins[idx].state == PluginState::Active {
            let settings_json = serde_json::to_string(&self.plugins[idx].settings)?;
            let safe_id = serde_json::to_string(id)?;
            if let Some(sb) = &mut self.sandbox {
                let _ = sb.eval(&format!(
                    "if (__zero_plugins[{safe_id}] && __zero_plugins[{safe_id}].__applySettings) {{ __zero_plugins[{safe_id}].__applySettings({settings_json}); }}"
                ));
            }
        }
        Ok(())
    }

    /// Runs active plugins with `MessageHooks` permission on outgoing message content.
    /// Any plugin error or infinite loop is isolated into `PluginState::Failed`.
    pub fn dispatch_before_send(&mut self, content: &str) -> String {
        if self.sandbox.is_none() || self.active_count() == 0 {
            return content.to_string();
        }

        let mut current = content.to_string();
        let ids: Vec<(String, bool)> = self
            .plugins
            .iter()
            .filter(|p| p.state == PluginState::Active)
            .map(|p| {
                (
                    p.manifest.id.clone(),
                    p.manifest
                        .permissions
                        .contains(&PluginPermission::MessageHooks),
                )
            })
            .collect();

        for (id, has_hook_perm) in ids {
            if !has_hook_perm {
                continue;
            }
            let Ok(safe_id) = serde_json::to_string(&id) else {
                continue;
            };
            let Ok(safe_content) = serde_json::to_string(&current) else {
                continue;
            };
            let script = format!(
                "(function() {{\n\
                    var p = __zero_plugins[{safe_id}];\n\
                    if (p && typeof p.onBeforeSend === 'function') {{\n\
                        var out = p.onBeforeSend({safe_content});\n\
                        return (typeof out === 'string') ? out : {safe_content};\n\
                    }}\n\
                    return {safe_content};\n\
                }})();"
            );

            let res = {
                let Some(sb) = self.sandbox.as_mut() else {
                    break;
                };
                sb.eval(&script).and_then(|val| {
                    val.as_string()
                        .map(|s| s.to_std_string_escaped())
                        .ok_or_else(|| PluginError::Js("Expected string return".into()))
                })
            };

            match res {
                Ok(updated) => current = updated,
                Err(e) => {
                    let msg = e.to_string();
                    if let Some(p) = self.plugins.iter_mut().find(|p| p.manifest.id == id) {
                        p.state = PluginState::Failed(msg.clone());
                    }
                    self.push_log(format!("Isolated crash in plugin '{id}': {msg}"));
                }
            }
        }

        current
    }

    pub fn export_config(&self) -> PersistedPluginConfig {
        let mut cfg = PersistedPluginConfig::default();
        for p in &self.plugins {
            if p.state == PluginState::Active {
                cfg.enabled_ids.push(p.manifest.id.clone());
            }
            cfg.settings_by_plugin
                .insert(p.manifest.id.clone(), p.settings.clone());
        }
        cfg
    }

    pub fn save_config(&self, path: &Path) -> Result<(), PluginError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.export_config())?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn default_config_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("org", "DiscordZero", "DiscordZero")
            .map(|d| d.config_dir().join("plugins.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manifest(id: &str, perms: Vec<PluginPermission>) -> PluginManifest {
        PluginManifest {
            id: id.into(),
            name: id.into(),
            version: "1.0.0".into(),
            description: "test".into(),
            authors: vec!["Tester".into()],
            permissions: perms,
            compatibility: CompatibilityStatus::Native,
            compatibility_note: "Native".into(),
            settings_schema: vec![],
        }
    }

    #[test]
    fn sandbox_is_lazy_until_plugin_enabled() {
        let mut mgr = PluginManager::new();
        mgr.register_plugin(
            test_manifest("echo", vec![PluginPermission::MessageHooks]),
            "var __zero_last_plugin = { onBeforeSend: function(s) { return s + '!'; } };",
        );
        assert!(!mgr.is_sandbox_loaded());

        mgr.enable_plugin("echo").unwrap();
        assert!(mgr.is_sandbox_loaded());
        assert_eq!(mgr.dispatch_before_send("hello"), "hello!");

        mgr.disable_plugin("echo").unwrap();
        assert!(!mgr.is_sandbox_loaded());
    }

    #[test]
    fn infinite_loop_in_plugin_is_isolated_without_hanging_host() {
        let mut mgr = PluginManager::new();
        mgr.register_plugin(
            test_manifest("bad-loop", vec![PluginPermission::MessageHooks]),
            "var __zero_last_plugin = { onBeforeSend: function(s) { while(true) {} return s; } };",
        );
        mgr.enable_plugin("bad-loop").unwrap();

        let out = mgr.dispatch_before_send("safe message");
        assert_eq!(out, "safe message");
        assert!(matches!(mgr.plugins()[0].state, PluginState::Failed(_)));
    }

    #[test]
    fn permission_boundary_blocks_unpermitted_message_hook() {
        let mut mgr = PluginManager::new();
        mgr.register_plugin(
            test_manifest("no-hook-perm", vec![PluginPermission::Logging]),
            "var __zero_last_plugin = { onBeforeSend: function(s) { return 'HIJACKED'; } };",
        );
        mgr.enable_plugin("no-hook-perm").unwrap();
        assert_eq!(mgr.dispatch_before_send("hello"), "hello");
    }
}
