#![forbid(unsafe_code)]

use zero_plugins::{
    CompatibilityStatus, PluginManager, PluginManifest, PluginPermission, SettingDef, SettingType,
    SettingValue,
};

/// JavaScript compatibility shim implementing Vencord's `definePlugin`,
/// `definePluginSettings`, `OptionType`, `FluxDispatcher`, and `@api/Commands`
/// inside `zero-plugins`.
pub const VENCORD_PRELUDE_JS: &str = r#"
var OptionType = {
    BOOLEAN: 0,
    STRING: 1,
    NUMBER: 2,
    SELECT: 3
};

var ApplicationCommandInputType = {
    BUILT_IN: 0,
    BUILT_IN_TEXT: 1,
    BUILT_IN_INTEGRATION: 2,
    BOT: 3,
    PLACEHOLDER: 4
};

var __zero_commands = {};
var __zero_flux_listeners = {};

var FluxDispatcher = {
    subscribe: function(eventType, fn) {
        if (typeof fn !== "function") return;
        if (!__zero_flux_listeners[eventType]) {
            __zero_flux_listeners[eventType] = [];
        }
        __zero_flux_listeners[eventType].push(fn);
    },
    unsubscribe: function(eventType, fn) {
        var list = __zero_flux_listeners[eventType];
        if (!list) return;
        __zero_flux_listeners[eventType] = list.filter(function(f) { return f !== fn; });
    },
    dispatch: function(action) {
        if (!action || !action.type) return;
        var list = __zero_flux_listeners[action.type];
        if (!list) return;
        for (var i = 0; i < list.length; i++) {
            list[i](action);
        }
    }
};

function __zero_dispatch_command(name, argText) {
    var cmd = __zero_commands[name];
    if (!cmd || typeof cmd.execute !== "function") return "";
    var args = argText ? [{ name: "text", value: argText }] : [];
    var res = cmd.execute(args, {});
    if (typeof res === "string") return res;
    if (res && typeof res.content === "string") return res.content;
    return "";
}

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
    var registeredCmds = [];
    var registeredFlux = [];
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
            if (Array.isArray(def.commands)) {
                for (var i = 0; i < def.commands.length; i++) {
                    var c = def.commands[i];
                    if (c && c.name) {
                        __zero_commands[c.name] = c;
                        registeredCmds.push(c.name);
                    }
                }
            }
            if (def.flux && typeof def.flux === "object") {
                for (var evt in def.flux) {
                    if (Object.prototype.hasOwnProperty.call(def.flux, evt)) {
                        var handler = def.flux[evt];
                        FluxDispatcher.subscribe(evt, handler);
                        registeredFlux.push([evt, handler]);
                    }
                }
            }
            if (typeof def.start === "function") {
                def.start.call(this);
            }
        },
        stop: function() {
            preSendListeners = [];
            for (var i = 0; i < registeredCmds.length; i++) {
                delete __zero_commands[registeredCmds[i]];
            }
            registeredCmds = [];
            for (var j = 0; j < registeredFlux.length; j++) {
                FluxDispatcher.unsubscribe(registeredFlux[j][0], registeredFlux[j][1]);
            }
            registeredFlux = [];
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
/// Litecord adapter or requires Discord's Electron/Webpack internals.
pub fn analyze_vencord_source(source: &str) -> VencordAnalysis {
    let internal_markers = [
        ("patches:", "Webpack module patching (`patches: [...]`)"),
        ("@webpack", "Discord Webpack module lookup (`@webpack`)"),
        ("findByProps", "Webpack `findByProps`"),
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
            reason: "Compatible via zero-vencord (`definePlugin`, `definePluginSettings`, `commands`, `FluxDispatcher`)."
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

pub fn install_custom_vencord_plugin(
    mgr: &mut PluginManager,
    id: impl Into<String>,
    name: impl Into<String>,
    source: impl Into<String>,
) -> VencordAnalysis {
    let src = source.into();
    let analysis = analyze_vencord_source(&src);
    mgr.register_plugin(
        PluginManifest {
            id: id.into(),
            name: name.into(),
            version: "1.0.0".into(),
            description: "User-imported Vencord/JS plugin".into(),
            authors: vec!["Custom".into()],
            permissions: vec![
                PluginPermission::MessageHooks,
                PluginPermission::SettingsReadWrite,
                PluginPermission::Logging,
            ],
            compatibility: analysis.status,
            compatibility_note: analysis.reason.clone(),
            settings_schema: vec![],
        },
        src,
    );
    analysis
}

pub fn register_builtin_vencord_plugins(mgr: &mut PluginManager) {
    mgr.set_prelude(VENCORD_PRELUDE_JS);

    // 1. TextReplace (Vencord Adapter)
    let text_replace_src = r#"
var settings = definePluginSettings({
    expandShrug: { type: OptionType.BOOLEAN, description: "Replace :shrug:", default: true },
    expandTableflip: { type: OptionType.BOOLEAN, description: "Replace :tableflip:", default: true },
    expandLenny: { type: OptionType.BOOLEAN, description: "Replace :lenny:", default: true }
});

definePlugin({
    name: "TextReplace",
    description: "Expands :shrug:, :tableflip:, :lenny:, and :sparkles: before sending.",
    settings: settings,
    onBeforeSend: function(content) {
        var out = content;
        if (settings.store.expandShrug) {
            out = out.split(":shrug:").join("\u00AF\\_(\u30C4)_/\u00AF");
        }
        if (settings.store.expandTableflip) {
            out = out.split(":tableflip:").join("(\u256F\u00B0\u25A1\u00B0)\u256F\uFE35 \u253B\u2501\u253B");
        }
        if (settings.store.expandLenny) {
            out = out.split(":lenny:").join("( \u0361\u00B0 \u035C\u0296 \u0361\u00B0)");
        }
        return out.split(":sparkles:").join("\u2728");
    }
});
"#;
    let analysis_tr = analyze_vencord_source(text_replace_src);
    mgr.register_plugin(
        PluginManifest {
            id: "vencord.text-replace".into(),
            name: "TextReplace (Vencord Adapter)".into(),
            version: "1.1.0".into(),
            description: "Expands :shrug:, :tableflip:, :lenny:, and :sparkles: in messages."
                .into(),
            authors: vec!["Litecord / Vencord Compat".into()],
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
                SettingDef {
                    key: "expandLenny".into(),
                    description: "Replace :lenny: with ( ͡° ͜ʖ ͡°)".into(),
                    kind: SettingType::Boolean,
                    default: SettingValue::Boolean(true),
                },
            ],
        },
        text_replace_src,
    );

    // 2. VencordCommands (/shrug, /tableflip, /lenny, /mock)
    let commands_src = r#"
definePlugin({
    name: "VencordCommands",
    description: "Adds /shrug, /tableflip, /lenny, and /mock <text> slash commands.",
    commands: [
        {
            name: "shrug",
            execute: function(args) {
                var extra = args.length ? args[0].value + " " : "";
                return { content: extra + "\u00AF\\_(\u30C4)_/\u00AF" };
            }
        },
        {
            name: "tableflip",
            execute: function(args) {
                var extra = args.length ? args[0].value + " " : "";
                return { content: extra + "(\u256F\u00B0\u25A1\u00B0)\u256F\uFE35 \u253B\u2501\u253B" };
            }
        },
        {
            name: "lenny",
            execute: function() { return { content: "( \u0361\u00B0 \u035C\u0296 \u0361\u00B0)" }; }
        },
        {
            name: "mock",
            execute: function(args) {
                var t = args.length ? args[0].value : "mock";
                var out = "";
                for (var i = 0; i < t.length; i++) {
                    out += (i % 2 === 1) ? t.charAt(i).toUpperCase() : t.charAt(i).toLowerCase();
                }
                return { content: out };
            }
        }
    ]
});
"#;
    let analysis_cmd = analyze_vencord_source(commands_src);
    mgr.register_plugin(
        PluginManifest {
            id: "vencord.commands".into(),
            name: "VencordCommands (/shrug, /mock)".into(),
            version: "1.0.0".into(),
            description: "Adds /shrug, /tableflip, /lenny, and /mock <text> slash commands.".into(),
            authors: vec!["Litecord / Vencord Compat".into()],
            permissions: vec![PluginPermission::MessageHooks],
            compatibility: analysis_cmd.status,
            compatibility_note: analysis_cmd.reason,
            settings_schema: vec![],
        },
        commands_src,
    );

    // 3. AutoCapitalize
    let auto_cap_src = r#"
var settings = definePluginSettings({
    enabled: { type: OptionType.BOOLEAN, description: "Capitalize first letter", default: true }
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
            authors: vec!["Litecord / Vencord Compat".into()],
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

    // 4. FluxMessageLogger (tests FluxDispatcher.subscribe("MESSAGE_CREATE"))
    let flux_src = r#"
var __flux_msg_count = 0;
definePlugin({
    name: "FluxMessageCounter",
    description: "Subscribes to FluxDispatcher MESSAGE_CREATE events in the sandbox.",
    flux: {
        MESSAGE_CREATE: function(evt) {
            __flux_msg_count += 1;
            __zero_log("Flux MESSAGE_CREATE #" + __flux_msg_count);
        }
    }
});
"#;
    let analysis_flux = analyze_vencord_source(flux_src);
    mgr.register_plugin(
        PluginManifest {
            id: "vencord.flux-counter".into(),
            name: "FluxMessageCounter (Flux Shim)".into(),
            version: "1.0.0".into(),
            description: "Listens to MESSAGE_CREATE events via the FluxDispatcher shim.".into(),
            authors: vec!["Litecord / Vencord Compat".into()],
            permissions: vec![PluginPermission::Logging],
            compatibility: analysis_flux.status,
            compatibility_note: analysis_flux.reason,
            settings_schema: vec![],
        },
        flux_src,
    );

    // 5. ShowHiddenChannels (Webpack Ref — honestly blocked)
    let webpack_example_src = r#"
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
    fn vencord_commands_and_flux_dispatcher_work_in_sandbox() {
        let mut mgr = PluginManager::new();
        register_builtin_vencord_plugins(&mut mgr);
        mgr.enable_plugin("vencord.commands").unwrap();
        mgr.enable_plugin("vencord.flux-counter").unwrap();

        assert_eq!(
            mgr.dispatch_command("mock", "hello").as_deref(),
            Some("hElLo")
        );
        assert_eq!(
            mgr.dispatch_command("shrug", "oops").as_deref(),
            Some("oops ¯\\_(ツ)_/¯")
        );
        mgr.emit_event("MESSAGE_CREATE", "{\"content\":\"hi\"}");
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
