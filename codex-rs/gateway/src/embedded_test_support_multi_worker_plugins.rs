pub(crate) fn write_embedded_plugin_fixture(
    codex_home: &std::path::Path,
    embedded_mcp_oauth_url: &str,
) {
    let plugin_root = codex_home.join("plugins/demo-plugin");
    std::fs::create_dir_all(codex_home.join(".git")).expect(".git should be created");
    std::fs::create_dir_all(codex_home.join(".agents/plugins"))
        .expect("marketplace dir should be created");
    std::fs::create_dir_all(plugin_root.join(".codex-plugin"))
        .expect("plugin manifest dir should be created");
    std::fs::write(
        codex_home.join(".agents/plugins/marketplace.json"),
        r#"{
  "name": "local",
  "plugins": [
    {
      "name": "demo-plugin",
      "source": {
        "source": "local",
        "path": "./plugins/demo-plugin"
      },
      "policy": {
        "installation": "AVAILABLE",
        "authentication": "ON_INSTALL"
      }
    }
  ]
}"#,
    )
    .expect("marketplace.json should be written");
    std::fs::write(
        plugin_root.join(".codex-plugin/plugin.json"),
        r##"{
  "name": "demo-plugin",
  "description": "Demo plugin detail",
  "interface": {
    "displayName": "Demo Plugin",
    "shortDescription": "Demo plugin short description",
    "longDescription": "Demo plugin long description",
    "developerName": "OpenAI",
    "category": "tools",
    "capabilities": ["commands"],
    "websiteURL": "https://openai.com/",
    "privacyPolicyURL": null,
    "termsOfServiceURL": null,
    "defaultPrompt": null,
    "brandColor": null,
    "composerIcon": null,
    "logo": null,
    "screenshots": []
  }
}"##,
    )
    .expect("plugin.json should be written");
    std::fs::write(
        plugin_root.join(".mcp.json"),
        format!(
            r#"{{
  "mcpServers": {{
    "demo-mcp": {{
      "type": "http",
      "url": "{embedded_mcp_oauth_url}/mcp",
      "oauth": {{
        "clientId": "demo-client-id"
      }}
    }}
  }}
}}"#
        ),
    )
    .expect(".mcp.json should be written");
}
