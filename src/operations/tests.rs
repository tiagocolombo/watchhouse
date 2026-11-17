use super::*;
#[test]
fn gateway_policy_rejects_redirect_credentials_and_unbounded_authority() {
    let mut p:Policy=serde_json::from_value(serde_json::json!({"schema_version":1,"revision":1,"service":"checkout","environment":"test","repository":"example/repo","gateway":"https://operations.example.invalid","token_env":"WATCHHOUSE_OPERATION_TOKEN","paused":false,"request_seconds":5,"action_seconds":60,"max_actions":2,"runbooks":[],"observation_ids":["health"]})).unwrap();
    assert!(p.validate().is_ok());
    for bad in [
        "http://operations.example.invalid",
        "https://user:secret@operations.example.invalid",
        "https://operations.example.invalid/elsewhere",
        "https://operations.example.invalid?token=secret",
        "https://operations.example.invalid:8443",
    ] {
        p.gateway = bad.into();
        assert!(p.validate().is_err());
    }
    p.gateway = "https://operations.example.invalid".into();
    p.observation_ids.push("../secret".into());
    assert!(p.validate().is_err());
}
