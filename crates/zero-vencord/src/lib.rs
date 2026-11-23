#![forbid(unsafe_code)]

use zero_plugins::{
    CompatibilityStatus, PluginManager, PluginManifest, PluginPermission, SettingDef, SettingType,
    SettingValue,
};

/// JavaScript compatibility shim implementing Vencord's `definePlugin`,
/// `definePluginSettings`, `OptionType`, and message pre-send hooks inside `zero-plugins`.
pub const VENCORD_PRELUDE_JS: &str = r#"
var OptionType = {
    BOOLEAN: 0,
    STRING: 1,
    NUMBER: 2,
    SELECT: 3
};

function definePluginSettings(schema) {
    var store = {};
    for (var k in schema) {
        if (Object.prototype.hasOwnProperty.call(schema, k)) {
            store[k] = schema[k].default;
        }
    }
    return {
        store: store,
        __schema: schema,
        __apply: function(next) {
            for (var k in next) {
                if (Object.prototype.hasOwnProperty.call(next, k)) {
                    var v = next[k];
                    if (v && typeof v === "object") {
                        if ("Boolean" in v) store[k] = v.Boolean;
                        else if ("String" in v) store[k] = v.String;
                        else if ("Integer" in v) store[k] = v.Integer;
                    } else {
                        store[k] = v;
                    }
                }
            }
        }
    };
}

function definePlugin(def) {
    var preSendListeners = [];
    var plugin = {
        name: def.name || "UnnamedPlugin",
        description: def.description || "",
        settings: def.settings || null,
        __applySettings: function(obj) {
            if (this.settings && typeof this.settings.__apply === "function") {
                this.settings.__apply(obj);
            }
        },
        addMessagePreSendListener: function(fn) {
            if (typeof fn === "function") preSendListeners.push(fn);
        },
        start: function() {
            if (typeof def.start === "function") {
                def.start.call(this);
            }
        },
        stop: function() {
            preSendListeners = [];
            if (typeof def.stop === "function") {
                def.stop.call(this);
            }
        },
        onBeforeSend: function(content) {
            var current = content;
            if (typeof def.onBeforeSend === "function") {
                var r = def.onBeforeSend.call(this, current);
                if (typeof r === "string") current = r;
            }
            for (var i = 0; i < preSendListeners.length; i++) {
                var out = preSendListeners[i](current);
                if (typeof out === "string") current = out;
            }
            return current;
        }
    };
    __zero_last_plugin = plugin;
    return plugin;
}
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VencordAnalysis {
    pub status: CompatibilityStatus,
    pub reason: String,
    pub detected_internals: Vec<&'static str>,
}

/// Inspects Vencord plugin source code to determine whether it can run via the
/// Discord Zero adapter or requires Discord's Electron/Webpack internals.
pub fn analyze_vencord_source(source: &str) -> VencordAnalysis {
    let internal_markers = [
        ("patches:", "Webpack module patching (`patches: [...]`)"),
        ("@webpack", "Discord Webpack module lookup (`@webpack`)"),
        ("findByProps", "Webpack `findByProps`"),
        ("FluxDispatcher", "Discord internal `FluxDispatcher`"),
        ("React.", "Discord React DOM injection"),
    ];

    let mut detected = Vec::new();
    for (needle, label) in internal_markers {
        if source.contains(needle) {
            detected.push(label);
        }
    }

    if !detected.is_empty() {
        return VencordAnalysis {
            status: CompatibilityStatus::RequiresDiscordInternals,
            reason: format!(
                "Requires Discord JS internals unavailable in native Rust: {}",
                detected.join(", ")
            ),
            detected_internals: detected,
        };
    }

    if source.contains("definePlugin(") || source.contains("definePluginSettings(") {
        return VencordAnalysis {
            status: CompatibilityStatus::VencordAdapter,
            reason: "Compatible via zero-vencord `definePlugin` / `definePluginSettings` adapter."
                .into(),
            detected_internals: Vec::new(),
        };
    }

    VencordAnalysis {
        status: CompatibilityStatus::Native,
        reason: "Runs directly in sandboxed zero-plugins runtime.".into(),
        detected_internals: Vec::new(),
    }
}

pub fn register_builtin_vencord_plugins(mgr: &mut PluginManager) {
    mgr.set_prelude(VENCORD_PRELUDE_JS);

    // 1. Vencord-style TextReplace plugin (works via zero-vencord adapter)
    let text_replace_src = r#"
var settings = definePluginSettings({
    expandShrug: {
        type: OptionType.BOOLEAN,
        description: "Replace :shrug: with kaomoji",
        default: true
    },
    expandTableflip: {
        type: OptionType.BOOLEAN,
        description: "Replace :tableflip: with kaomoji",
        default: true
    }
});

definePlugin({
    name: "TextReplace",
    description: "Expands common chat shortcuts (:shrug:, :tableflip:) before sending.",
    settings: settings,
    start: function() {
        __zero_log("TextReplace started");
    },
    onBeforeSend: function(content) {
        var out = content;
        if (settings.store.expandShrug) {
            out = out.split(":shrug:").join("\u00AF\\_(\u30C4)_/\u00AF");
        }
        if (settings.store.expandTableflip) {
            out = out.split(":tableflip:").join("(\u256F\u00B0\u25A1\u00B0)\u256F\uFE35 \u253B\u2501\u253B");
        }
        return out;
    }
});
"#;

    let analysis_tr = analyze_vencord_source(text_replace_src);
    mgr.register_plugin(
        PluginManifest {
            id: "vencord.text-replace".into(),
            name: "TextReplace (Vencord Adapter)".into(),
            version: "1.0.0".into(),
            description: "Expands :shrug: and :tableflip: in outgoing messages.".into(),
            authors: vec!["Discord Zero / Vencord Compat".into()],
            permissions: vec![
                PluginPermission::MessageHooks,
                PluginPermission::SettingsReadWrite,
                PluginPermission::Logging,
            ],
            compatibility: analysis_tr.status,
            compatibility_note: analysis_tr.reason,
            settings_schema: vec![
                SettingDef {
                    key: "expandShrug".into(),
                    description: "Replace :shrug: with ¯\\_(ツ)_/¯".into(),
                    kind: SettingType::Boolean,
                    default: SettingValue::Boolean(true),
                },
                SettingDef {
                    key: "expandTableflip".into(),
                    description: "Replace :tableflip: with (╯°□°)╯︵ ┻━┻".into(),
                    kind: SettingType::Boolean,
                    default: SettingValue::Boolean(true),
                },
            ],
        },
        text_replace_src,
    );

    // 2. Vencord-style AutoCapitalize plugin
    let auto_cap_src = r#"
var settings = definePluginSettings({
    enabled: {
        type: OptionType.BOOLEAN,
        description: "Capitalize first letter of outgoing messages",
        default: true
    }
});

definePlugin({
    name: "AutoCapitalize",
    description: "Capitalizes the first character of outgoing messages.",
    settings: settings,
    onBeforeSend: function(content) {
        if (!settings.store.enabled || !content || content.length === 0) return content;
        return content.charAt(0).toUpperCase() + content.slice(1);
    }
});
"#;

    let analysis_ac = analyze_vencord_source(auto_cap_src);
    mgr.register_plugin(
        PluginManifest {
            id: "vencord.auto-capitalize".into(),
            name: "AutoCapitalize (Vencord Adapter)".into(),
            version: "1.0.0".into(),
            description: "Capitalizes the first character of outgoing messages.".into(),
            authors: vec!["Discord Zero / Vencord Compat".into()],
            permissions: vec![
                PluginPermission::MessageHooks,
                PluginPermission::SettingsReadWrite,
            ],
            compatibility: analysis_ac.status,
            compatibility_note: analysis_ac.reason,
            settings_schema: vec![SettingDef {
                key: "enabled".into(),
                description: "Capitalize first letter".into(),
                kind: SettingType::Boolean,
                default: SettingValue::Boolean(true),
            }],
        },
        auto_cap_src,
    );

    // 3. Representative Webpack-dependent Vencord plugin (honestly classified as blocked)
    let webpack_example_src = r#"
// Representative Vencord Webpack plugin pattern
definePlugin({
    name: "ShowHiddenChannels",
    description: "Requires Discord internal Webpack channel/permission stores.",
    patches: [
        { find: "canBeAccessedByUser", replacement: { match: /return !1/, replace: "return !0" } }
    ]
});
"#;
    let analysis_wp = analyze_vencord_source(webpack_example_src);
    mgr.register_plugin(
        PluginManifest {
            id: "vencord.show-hidden-channels".into(),
            name: "ShowHiddenChannels (Webpack)".into(),
            version: "1.0.0".into(),
            description: "Demonstrates compatibility detection for Webpack-patched plugins.".into(),
            authors: vec!["Vencord Upstream Pattern".into()],
            permissions: vec![],
            compatibility: analysis_wp.status,
            compatibility_note: analysis_wp.reason,
            settings_schema: vec![],
        },
        webpack_example_src,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use zero_plugins::PluginState;

    #[test]
    fn vencord_adapter_runs_text_replace_and_respects_settings() {
        let mut mgr = PluginManager::new();
        register_builtin_vencord_plugins(&mut mgr);
        assert!(!mgr.is_sandbox_loaded());

        mgr.enable_plugin("vencord.text-replace").unwrap();
        assert_eq!(
            mgr.dispatch_before_send("hello :shrug:"),
            "hello ¯\\_(ツ)_/¯"
        );

        mgr.set_plugin_setting(
            "vencord.text-replace",
            "expandShrug",
            SettingValue::Boolean(false),
        )
        .unwrap();
        assert_eq!(mgr.dispatch_before_send("hello :shrug:"), "hello :shrug:");
    }

    #[test]
    fn webpack_dependent_plugin_is_honestly_blocked() {
        let mut mgr = PluginManager::new();
        register_builtin_vencord_plugins(&mut mgr);

        let err = mgr
            .enable_plugin("vencord.show-hidden-channels")
            .unwrap_err();
        assert!(err.to_string().contains("blocked"));
        let p = mgr
            .plugins()
            .iter()
            .find(|p| p.manifest.id == "vencord.show-hidden-channels")
            .unwrap();
        assert!(matches!(p.state, PluginState::Failed(_)));
    }
}
