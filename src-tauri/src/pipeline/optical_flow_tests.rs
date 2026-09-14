use super::optical_flow::{optical_flow_enabled, optical_flow_status};

#[test]
fn optical_flow_is_permanently_disabled() {
    assert!(!optical_flow_enabled());
}

#[test]
fn optical_flow_status_documents_deferral() {
    let status = optical_flow_status();
    assert!(
        status.contains("optical flow") || status.contains("optical-flow"),
        "status should mention optical flow: {status}"
    );
}
