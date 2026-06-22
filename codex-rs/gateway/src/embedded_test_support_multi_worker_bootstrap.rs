pub(crate) fn paginated_model_list_result(
    request: &codex_app_server_protocol::JSONRPCRequest,
    models: &[(&'static str, &'static str, bool)],
) -> serde_json::Value {
    let (start, end, next_cursor) =
        paginated_mock_bounds(request, models.len(), "worker-model-offset:", "model/list");
    serde_json::json!({
        "data": models[start..end].iter().map(|(id, display_name, is_default)| serde_json::json!({
            "id": id,
            "model": id,
            "upgrade": null,
            "upgradeInfo": null,
            "availabilityNux": null,
            "displayName": display_name,
            "description": format!("{display_name} description"),
            "hidden": false,
            "supportedReasoningEfforts": [{
                "reasoningEffort": "medium",
                "description": "Balanced"
            }],
            "defaultReasoningEffort": "medium",
            "inputModalities": ["text"],
            "supportsPersonality": false,
            "additionalSpeedTiers": [],
            "isDefault": is_default,
        })).collect::<Vec<_>>(),
        "nextCursor": next_cursor,
    })
}

pub(crate) fn paginated_mock_bounds(
    request: &codex_app_server_protocol::JSONRPCRequest,
    total_len: usize,
    cursor_prefix: &str,
    method: &str,
) -> (usize, usize, Option<String>) {
    let cursor = request
        .params
        .as_ref()
        .and_then(|params| params.get("cursor"))
        .and_then(serde_json::Value::as_str);
    let offset = match cursor {
        Some(cursor) => cursor
            .strip_prefix(cursor_prefix)
            .unwrap_or_else(|| panic!("unexpected {method} cursor: {cursor}"))
            .parse::<usize>()
            .unwrap_or_else(|err| panic!("invalid {method} cursor {cursor}: {err}")),
        None => 0,
    };
    let start = offset.min(total_len);
    let end = start.saturating_add(1).min(total_len);
    let next_cursor = (end < total_len).then(|| format!("{cursor_prefix}{end}"));
    (start, end, next_cursor)
}

pub(crate) fn multi_worker_fuzzy_file_search_response(worker_label: &str) -> serde_json::Value {
    let (root, path, file_name, score, indices) = match worker_label {
        "worker-a" => (
            "/tmp/worker-a-primary",
            "docs/gateway.md",
            "gateway.md",
            42,
            vec![5, 6, 7, 8],
        ),
        "worker-b" => (
            "/tmp/worker-b-search",
            "src/gateway.rs",
            "gateway.rs",
            60,
            vec![4, 5, 6, 7],
        ),
        other => panic!("unexpected worker label: {other}"),
    };
    serde_json::json!({
        "files": [{
            "root": root,
            "path": path,
            "match_type": "file",
            "file_name": file_name,
            "score": score,
            "indices": indices,
        }],
    })
}

pub(crate) fn paginated_apps_list_result(
    request: &codex_app_server_protocol::JSONRPCRequest,
    apps: &[(&'static str, &'static str)],
) -> serde_json::Value {
    let (start, end, next_cursor) =
        paginated_mock_bounds(request, apps.len(), "worker-app-offset:", "app/list");
    serde_json::json!({
        "data": apps[start..end].iter().map(|(id, name)| serde_json::json!({
            "id": id,
            "name": name,
            "description": format!("{name} description"),
            "logoUrl": null,
            "logoUrlDark": null,
            "distributionChannel": null,
            "branding": null,
            "appMetadata": null,
            "labels": null,
            "installUrl": null,
            "isAccessible": false,
            "isEnabled": true,
            "pluginDisplayNames": [],
        })).collect::<Vec<_>>(),
        "nextCursor": next_cursor,
    })
}

pub(crate) fn paginated_mcp_server_status_list_result(
    request: &codex_app_server_protocol::JSONRPCRequest,
    names: &[&'static str],
) -> serde_json::Value {
    let (start, end, next_cursor) = paginated_mock_bounds(
        request,
        names.len(),
        "worker-mcp-status-offset:",
        "mcpServerStatus/list",
    );
    serde_json::json!({
        "data": names[start..end].iter().map(|name| serde_json::json!({
            "name": name,
            "tools": {},
            "resources": [],
            "resourceTemplates": [],
            "authStatus": "bearerToken",
        })).collect::<Vec<_>>(),
        "nextCursor": next_cursor,
    })
}

pub(crate) fn paginated_experimental_feature_list_result(
    request: &codex_app_server_protocol::JSONRPCRequest,
    names: &[&'static str],
) -> serde_json::Value {
    let (start, end, next_cursor) = paginated_mock_bounds(
        request,
        names.len(),
        "worker-experimental-feature-offset:",
        "experimentalFeature/list",
    );
    serde_json::json!({
        "data": names[start..end].iter().map(|name| serde_json::json!({
            "name": name,
            "stage": "beta",
            "displayName": format!("Gateway Test Feature {name}"),
            "description": format!("Used by gateway multi-worker passthrough tests for {name}"),
            "announcement": null,
            "enabled": false,
            "defaultEnabled": false,
        })).collect::<Vec<_>>(),
        "nextCursor": next_cursor,
    })
}
