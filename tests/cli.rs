use std::process::Command;

#[test]
fn offline_recommendation_returns_valid_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_local-ai-advisor"))
        .args(["recommend", "--offline", "--format", "json"])
        .output()
        .expect("binary should run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert!(json["recommendations"].is_array());
    assert!(json["hardware"].is_object());
    assert_eq!(json["estimates_are_approximate"], true);
}
