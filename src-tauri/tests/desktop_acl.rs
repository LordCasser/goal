//! Regression for application ACL activation: adding window permissions must
//! neither disable existing planner IPC nor grant Windows controls everywhere.
use serde_json::Value;

#[test]
fn generated_permissions_preserve_business_and_isolate_window_commands() {
    let manifest: Value = serde_json::from_str(include_str!(concat!(
        env!("OUT_DIR"),
        "/desktop-permissions.json"
    )))
    .unwrap();
    let permissions = manifest["permission"].as_array().unwrap();
    assert_eq!(permissions.len(), 4);
    let business = permissions
        .iter()
        .find(|p| p["identifier"] == "allow-planner-commands")
        .unwrap();
    let allowed = business["commands"]["allow"].as_array().unwrap();
    for command in [
        "get_planner_state",
        "get_settings",
        "set_theme",
        "set_show_relation_lines",
        "get_ai_settings",
        "resolve_coach_task_preview",
    ] {
        assert!(
            allowed.contains(&Value::from(command)),
            "existing IPC denied: {command}"
        );
    }
    assert!(allowed.iter().all(|value| {
        let command = value.as_str().unwrap();
        !command.contains('*') && !command.starts_with("desktop_")
    }));
    for command in [
        "desktop_shell_state",
        "desktop_shell_ready",
        "desktop_shell_regions",
    ] {
        let id = format!("allow-{}", command.replace('_', "-"));
        let permission = permissions.iter().find(|p| p["identifier"] == id).unwrap();
        assert_eq!(
            permission["commands"]["allow"],
            serde_json::json!([command])
        );
    }
    let common: Value = serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
    assert_eq!(common["windows"], serde_json::json!(["main"]));
    assert!(common.get("remote").is_none());
    assert!(common["permissions"]
        .as_array()
        .unwrap()
        .contains(&Value::from("allow-planner-commands")));
    assert!(common["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|p| !p.as_str().unwrap().contains("desktop-shell")));
}
