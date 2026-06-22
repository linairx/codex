use super::embedded_test_support_reconnect_common;
use super::*;

#[path = "embedded_test_support_multi_connection.rs"]
mod embedded_test_support_multi_connection;

#[path = "embedded_test_support_multi_connection_disconnect.rs"]
mod embedded_test_support_multi_connection_disconnect;

#[path = "embedded_test_support_mock_responses.rs"]
mod embedded_test_support_mock_responses;

#[path = "embedded_test_support_embedded_apps.rs"]
mod embedded_test_support_embedded_apps;

#[path = "embedded_test_support_websocket.rs"]
mod embedded_test_support_websocket;

#[path = "embedded_test_support_reconnect.rs"]
mod embedded_test_support_reconnect;

#[path = "embedded_test_support_reconnect_v2.rs"]
mod embedded_test_support_reconnect_v2;

#[path = "embedded_test_support_multi_worker.rs"]
mod embedded_test_support_multi_worker;

pub(crate) use self::embedded_test_support_embedded_apps::*;
pub(crate) use self::embedded_test_support_mock_responses::*;
pub(crate) use self::embedded_test_support_multi_worker::*;
#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_reconnect::*;
#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_reconnect_v2::*;
pub(crate) use self::embedded_test_support_websocket::*;

pub(super) fn write_embedded_plugin_fixture(
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
