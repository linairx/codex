use super::*;
use pretty_assertions::assert_eq;

#[test]
fn records_v2_connection_client_request_metrics_with_outcome_tags() {
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

    let mut saw_count = false;
    let mut saw_duration = false;
    let mut saw_pending_client_requests = false;
    let mut saw_max_pending_client_requests = false;
    let mut pending_client_request_worker_points = Vec::new();
    let mut pending_client_request_method_points = Vec::new();
    for metric in metrics {
        match metric.name() {
            name if name == V2_CONNECTION_COUNT_METRIC => {
                saw_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let point = sum.data_points().next().expect("count point");
                            assert_eq!(point.value(), 1);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected connection count aggregation"),
                    },
                    _ => panic!("unexpected connection count type"),
                }
            }
            name if name == V2_CONNECTION_DURATION_METRIC => {
                saw_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 14.0);
                            assert_eq!(
                                metric_attributes(point),
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected connection duration aggregation"),
                    },
                    _ => panic!("unexpected connection duration type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_CLIENT_REQUEST_METRIC => {
                saw_pending_client_requests = true;
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
                        _ => panic!("unexpected pending client request aggregation"),
                    },
                    _ => panic!("unexpected pending client request type"),
                }
            }
            name if name == V2_CONNECTION_MAX_PENDING_CLIENT_REQUEST_METRIC => {
                saw_max_pending_client_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 5.0);
                            assert_eq!(
                                metric_attributes(point),
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected max pending client request aggregation"),
                    },
                    _ => panic!("unexpected max pending client request type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_CLIENT_REQUEST_BY_METHOD_METRIC => {
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            pending_client_request_method_points.extend(
                                histogram.data_points().map(|point| {
                                    (metric_attributes(point), point.count(), point.sum())
                                }),
                            );
                        }
                        _ => panic!("unexpected pending client request by method aggregation"),
                    },
                    _ => panic!("unexpected pending client request by method type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_CLIENT_REQUEST_BY_WORKER_METRIC => {
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            pending_client_request_worker_points.extend(
                                histogram.data_points().map(|point| {
                                    (metric_attributes(point), point.count(), point.sum())
                                }),
                            );
                        }
                        _ => panic!("unexpected pending client request by worker aggregation"),
                    },
                    _ => panic!("unexpected pending client request by worker type"),
                }
            }
            _ => {}
        }
    }

    assert!(saw_count);
    assert!(saw_duration);
    assert!(saw_pending_client_requests);
    assert!(saw_max_pending_client_requests);
    pending_client_request_worker_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        pending_client_request_worker_points,
        vec![
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "2".to_string()),
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
                    ("worker_id".to_string(), "7".to_string()),
                ]),
                1,
                1.0,
            ),
        ]
    );
    pending_client_request_method_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        pending_client_request_method_points,
        vec![
            (
                BTreeMap::from([
                    ("method".to_string(), "command/exec".to_string()),
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                ]),
                1,
                2.0,
            ),
            (
                BTreeMap::from([
                    ("method".to_string(), "thread/read".to_string()),
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                ]),
                1,
                1.0,
            ),
        ]
    );
}
