use crate::providers::SafetyLevel;

pub enum DeleteDecision {
    Allow,
    NeedsConfirmation { reason: String },
    Reject { reason: String },
}

pub fn evaluate_delete(safety: &SafetyLevel, confirm_caution: bool) -> DeleteDecision {
    match safety {
        SafetyLevel::Safe => DeleteDecision::Allow,
        SafetyLevel::Caution => {
            if confirm_caution {
                DeleteDecision::Allow
            } else {
                DeleteDecision::NeedsConfirmation {
                    reason: "Caution-level item: may cause rebuilds or re-downloads. Set confirm_caution to true to proceed.".to_string(),
                }
            }
        }
        SafetyLevel::Unsafe => DeleteDecision::Reject {
            reason: "Unsafe item: contains configuration or state that cannot be re-created. Use the TUI (ccmd) for manual deletion.".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_items_always_allowed() {
        match evaluate_delete(&SafetyLevel::Safe, false) {
            DeleteDecision::Allow => {}
            _ => panic!("Safe items should always be allowed"),
        }
    }

    #[test]
    fn caution_items_rejected_without_confirmation() {
        match evaluate_delete(&SafetyLevel::Caution, false) {
            DeleteDecision::NeedsConfirmation { reason } => {
                assert!(reason.contains("caution") || reason.contains("Caution"));
            }
            _ => panic!("Caution items should need confirmation"),
        }
    }

    #[test]
    fn caution_items_allowed_with_confirmation() {
        match evaluate_delete(&SafetyLevel::Caution, true) {
            DeleteDecision::Allow => {}
            _ => panic!("Caution items should be allowed with confirmation"),
        }
    }

    #[test]
    fn unsafe_items_always_rejected() {
        match evaluate_delete(&SafetyLevel::Unsafe, false) {
            DeleteDecision::Reject { reason } => {
                assert!(reason.contains("TUI") || reason.contains("unsafe"));
            }
            _ => panic!("Unsafe items should always be rejected"),
        }
    }

    #[test]
    fn unsafe_items_rejected_even_with_confirmation() {
        match evaluate_delete(&SafetyLevel::Unsafe, true) {
            DeleteDecision::Reject { .. } => {}
            _ => panic!("Unsafe items should be rejected even with confirm_caution"),
        }
    }
}
