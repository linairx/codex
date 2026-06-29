mod support {
    pub(super) use super::super::*;
}

#[path = "v2_tests_cases_2_late_aggregation_catalog.rs"]
mod catalog;

#[path = "v2_tests_cases_2_late_aggregation_plugin.rs"]
mod plugin;

#[path = "v2_tests_cases_2_late_aggregation_account.rs"]
mod account;

#[path = "v2_tests_cases_2_late_aggregation_model.rs"]
mod model;

#[path = "v2_tests_cases_2_late_aggregation_capability.rs"]
mod capability;
