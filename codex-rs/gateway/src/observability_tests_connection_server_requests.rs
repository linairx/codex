use super::*;
use pretty_assertions::assert_eq;

#[test]
fn records_v2_connection_server_request_metrics_with_outcome_tags() {
    let observability = record_sample_v2_connection();
    let resource_metrics = observability
        .metrics
        .as_ref()
        .expect("metrics client")
        .snapshot()
        .expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_pending_server_requests = false;
    let mut saw_answered_but_unresolved_server_requests = false;
    let mut pending_server_request_worker_points = Vec::new();
    let mut answered_but_unresolved_server_request_worker_points = Vec::new();
    let mut server_request_backlog_worker_points = Vec::new();
    let mut saw_pending_server_requests_by_method = false;
    let mut saw_answered_but_unresolved_server_requests_by_method = false;
    let mut saw_server_request_backlog = false;
    let mut saw_max_server_request_backlog = false;
    let mut saw_server_request_backlog_by_method = false;
    for metric in metrics {
        match metric.name() {
            name if name == V2_CONNECTION_PENDING_SERVER_REQUEST_METRIC => {
                saw_pending_server_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 2.0);
                            assert_eq!(
                                metric_attributes(point),
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected pending server request aggregation"),
                    },
                    _ => panic!("unexpected pending server request type"),
                }
            }
            name if name == V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_METRIC => {
                saw_answered_but_unresolved_server_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 1.0);
                            assert_eq!(
                                metric_attributes(point),
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => {
                            panic!("unexpected answered-but-unresolved server request aggregation")
                        }
                    },
                    _ => panic!("unexpected answered-but-unresolved server request type"),
                }
            }
            name if name == V2_CONNECTION_SERVER_REQUEST_BACKLOG_METRIC => {
                saw_server_request_backlog = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 3.0);
                            assert_eq!(
                                metric_attributes(point),
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected server request backlog aggregation"),
                    },
                    _ => panic!("unexpected server request backlog type"),
                }
            }
            name if name == V2_CONNECTION_MAX_SERVER_REQUEST_BACKLOG_METRIC => {
                saw_max_server_request_backlog = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 7.0);
                            assert_eq!(
                                metric_attributes(point),
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected max server request backlog aggregation"),
                    },
                    _ => panic!("unexpected max server request backlog type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_SERVER_REQUEST_BY_WORKER_METRIC => {
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            pending_server_request_worker_points.extend(
                                histogram.data_points().map(|point| {
                                    (metric_attributes(point), point.count(), point.sum())
                                }),
                            );
                        }
                        _ => panic!("unexpected pending server request by worker aggregation"),
                    },
                    _ => panic!("unexpected pending server request by worker type"),
                }
            }
            name if name
                == V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_BY_WORKER_METRIC =>
            {
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            answered_but_unresolved_server_request_worker_points.extend(
                                histogram.data_points().map(|point| {
                                    (metric_attributes(point), point.count(), point.sum())
                                }),
                            );
                        }
                        _ => panic!(
                            "unexpected answered-but-unresolved server request by worker aggregation"
                        ),
                    },
                    _ => panic!("unexpected answered-but-unresolved server request by worker type"),
                }
            }
            name if name == V2_CONNECTION_SERVER_REQUEST_BACKLOG_BY_WORKER_METRIC => {
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            server_request_backlog_worker_points.extend(
                                histogram.data_points().map(|point| {
                                    (metric_attributes(point), point.count(), point.sum())
                                }),
                            );
                        }
                        _ => panic!("unexpected server request backlog by worker aggregation"),
                    },
                    _ => panic!("unexpected server request backlog by worker type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_SERVER_REQUEST_BY_METHOD_METRIC => {
                saw_pending_server_requests_by_method = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 2.0);
                            assert_eq!(
                                metric_attributes(point),
                                BTreeMap::from([
                                    (
                                        "outcome".to_string(),
                                        "downstream_session_ended".to_string(),
                                    ),
                                    (
                                        "method".to_string(),
                                        "item/tool/requestUserInput".to_string(),
                                    ),
                                ])
                            );
                        }
                        _ => panic!("unexpected pending server request by method aggregation"),
                    },
                    _ => panic!("unexpected pending server request by method type"),
                }
            }
            name if name
                == V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_BY_METHOD_METRIC =>
            {
                saw_answered_but_unresolved_server_requests_by_method = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 1.0);
                            assert_eq!(
                                metric_attributes(point),
                                BTreeMap::from([
                                    (
                                        "outcome".to_string(),
                                        "downstream_session_ended".to_string(),
                                    ),
                                    (
                                        "method".to_string(),
                                        "item/tool/requestUserInput".to_string(),
                                    ),
                                ])
                            );
                        }
                        _ => panic!(
                            "unexpected answered-but-unresolved server request by method aggregation"
                        ),
                    },
                    _ => panic!("unexpected answered-but-unresolved server request by method type"),
                }
            }
            name if name == V2_CONNECTION_SERVER_REQUEST_BACKLOG_BY_METHOD_METRIC => {
                saw_server_request_backlog_by_method = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 3.0);
                            assert_eq!(
                                metric_attributes(point),
                                BTreeMap::from([
                                    (
                                        "outcome".to_string(),
                                        "downstream_session_ended".to_string(),
                                    ),
                                    (
                                        "method".to_string(),
                                        "item/tool/requestUserInput".to_string(),
                                    ),
                                ])
                            );
                        }
                        _ => panic!("unexpected server request backlog by method aggregation"),
                    },
                    _ => panic!("unexpected server request backlog by method type"),
                }
            }
            _ => {}
        }
    }

    assert!(saw_pending_server_requests);
    assert!(saw_answered_but_unresolved_server_requests);
    assert!(saw_server_request_backlog);
    assert!(saw_max_server_request_backlog);
    pending_server_request_worker_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        pending_server_request_worker_points,
        vec![
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "3".to_string()),
                ]),
                1,
                2.0,
            ),
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "8".to_string()),
                ]),
                1,
                0.0,
            ),
        ]
    );
    answered_but_unresolved_server_request_worker_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        answered_but_unresolved_server_request_worker_points,
        vec![
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "3".to_string()),
                ]),
                1,
                0.0,
            ),
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "8".to_string()),
                ]),
                1,
                1.0,
            ),
        ]
    );
    server_request_backlog_worker_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        server_request_backlog_worker_points,
        vec![
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "3".to_string()),
                ]),
                1,
                2.0,
            ),
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "8".to_string()),
                ]),
                1,
                1.0,
            ),
        ]
    );
    assert!(saw_pending_server_requests_by_method);
    assert!(saw_answered_but_unresolved_server_requests_by_method);
    assert!(saw_server_request_backlog_by_method);
}
