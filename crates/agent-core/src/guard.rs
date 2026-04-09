use crate::tool::RiskLevel;

#[derive(Debug, Clone)]
pub enum Decision {
    Allow,
    Deny(String),
    NeedConfirm,
}

#[async_trait::async_trait]
pub trait Guard: Send + Sync {
    async fn check(&self, tool_name: &str, input: &serde_json::Value) -> Decision;
}

/// Always allows any tool call.
pub struct AutoGuard;

#[async_trait::async_trait]
impl Guard for AutoGuard {
    async fn check(&self, _tool_name: &str, _input: &serde_json::Value) -> Decision {
        Decision::Allow
    }
}

/// Routes by risk level: Low -> Allow, Medium | High -> NeedConfirm.
pub struct ConfirmGuard<F: Fn(&str) -> RiskLevel + Send + Sync> {
    risk_fn: F,
}

impl<F: Fn(&str) -> RiskLevel + Send + Sync> ConfirmGuard<F> {
    pub fn new(risk_fn: F) -> Self {
        Self { risk_fn }
    }
}

#[async_trait::async_trait]
impl<F: Fn(&str) -> RiskLevel + Send + Sync> Guard for ConfirmGuard<F> {
    async fn check(&self, tool_name: &str, _input: &serde_json::Value) -> Decision {
        match (self.risk_fn)(tool_name) {
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

        let decision = guard.check("any_tool", &input).await;
        assert!(matches!(decision, Decision::Allow));

        let decision2 = guard.check("dangerous_tool", &input).await;
        assert!(matches!(decision2, Decision::Allow));
    }

    #[tokio::test]
    async fn confirm_guard_low_risk_allows() {
        let guard = ConfirmGuard::new(|name: &str| {
            if name == "safe_tool" {
                RiskLevel::Low
            } else {
                RiskLevel::High
            }
        });

        let input = serde_json::json!({});
        let decision = guard.check("safe_tool", &input).await;
        assert!(matches!(decision, Decision::Allow));
    }

    #[tokio::test]
    async fn confirm_guard_medium_risk_needs_confirm() {
        let guard = ConfirmGuard::new(|_name: &str| RiskLevel::Medium);

        let input = serde_json::json!({});
        let decision = guard.check("medium_tool", &input).await;
        assert!(matches!(decision, Decision::NeedConfirm));
    }

    #[tokio::test]
    async fn confirm_guard_high_risk_needs_confirm() {
        let guard = ConfirmGuard::new(|_name: &str| RiskLevel::High);

        let input = serde_json::json!({});
        let decision = guard.check("risky_tool", &input).await;
        assert!(matches!(decision, Decision::NeedConfirm));
    }
}
