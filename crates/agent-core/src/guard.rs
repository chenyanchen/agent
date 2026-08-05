use crate::tool::RiskLevel;

#[derive(Debug, Clone)]
pub enum Decision {
    Allow,
    Deny(String),
    NeedConfirm,
}

#[async_trait::async_trait]
pub trait Guard: Send + Sync {
    async fn check(
        &self,
        tool_name: &str,
        risk_level: RiskLevel,
        input: &serde_json::Value,
    ) -> Decision;
}

/// Always allows any tool call.
pub struct AutoGuard;

#[async_trait::async_trait]
impl Guard for AutoGuard {
    async fn check(
        &self,
        _tool_name: &str,
        _risk_level: RiskLevel,
        _input: &serde_json::Value,
    ) -> Decision {
        Decision::Allow
    }
}

/// Routes by risk level: Low -> Allow, Medium | High -> NeedConfirm.
pub struct ConfirmGuard;

#[async_trait::async_trait]
impl Guard for ConfirmGuard {
    async fn check(
        &self,
        _tool_name: &str,
        risk_level: RiskLevel,
        _input: &serde_json::Value,
    ) -> Decision {
        match risk_level {
            RiskLevel::Low => Decision::Allow,
            RiskLevel::Medium | RiskLevel::High => Decision::NeedConfirm,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn auto_guard_always_allows() {
        let guard = AutoGuard;
        let input = serde_json::json!({});

        let decision = guard.check("any_tool", RiskLevel::Low, &input).await;
        assert!(matches!(decision, Decision::Allow));

        let decision2 = guard.check("dangerous_tool", RiskLevel::High, &input).await;
        assert!(matches!(decision2, Decision::Allow));
    }

    #[tokio::test]
    async fn confirm_guard_low_risk_allows() {
        let guard = ConfirmGuard;

        let input = serde_json::json!({});
        let decision = guard.check("safe_tool", RiskLevel::Low, &input).await;
        assert!(matches!(decision, Decision::Allow));
    }

    #[tokio::test]
    async fn confirm_guard_medium_risk_needs_confirm() {
        let guard = ConfirmGuard;

        let input = serde_json::json!({});
        let decision = guard.check("medium_tool", RiskLevel::Medium, &input).await;
        assert!(matches!(decision, Decision::NeedConfirm));
    }

    #[tokio::test]
    async fn confirm_guard_high_risk_needs_confirm() {
        let guard = ConfirmGuard;

        let input = serde_json::json!({});
        let decision = guard.check("risky_tool", RiskLevel::High, &input).await;
        assert!(matches!(decision, Decision::NeedConfirm));
    }
}
