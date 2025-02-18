use relay_common::time::UnixTimestamp;
use relay_dynamic_config::{MetricExtractionConfig, Options};
use relay_event_schema::protocol::{Event, Span};
use relay_metrics::Bucket;
use relay_quotas::DataCategory;

use crate::metrics_extraction::generic::{self, Extractable};
use crate::statsd::RelayTimers;

impl Extractable for Event {
    fn category(&self) -> DataCategory {
        // Obtain the event's data category, but treat default events as error events for the
        // purpose of metric tagging.
        match DataCategory::from(self.ty.value().copied().unwrap_or_default()) {
            DataCategory::Default => DataCategory::Error,
            category => category,
        }
    }

    fn timestamp(&self) -> Option<UnixTimestamp> {
        self.timestamp
            .value()
            .and_then(|ts| UnixTimestamp::from_datetime(ts.0))
    }
}

impl Extractable for Span {
    fn category(&self) -> DataCategory {
        DataCategory::Span
    }

    fn timestamp(&self) -> Option<UnixTimestamp> {
        self.timestamp
            .value()
            .and_then(|ts| UnixTimestamp::from_datetime(ts.0))
    }
}

/// Extracts metrics from an [`Event`].
///
/// The event must have a valid timestamp; if the timestamp is missing or invalid, no metrics are
/// extracted. Timestamp and clock drift correction should occur before metrics extraction to ensure
/// valid timestamps.
///
/// If this is a transaction event with spans, metrics will also be extracted from the spans.
pub fn extract_metrics(
    event: &Event,
    config: &MetricExtractionConfig,
    global_options: Option<&Options>,
) -> Vec<Bucket> {
    let mut metrics = generic::extract_metrics(event, config, global_options);

    relay_statsd::metric!(timer(RelayTimers::EventProcessingSpanMetricsExtraction), {
        if let Some(spans) = event.spans.value() {
            for annotated_span in spans {
                if let Some(span) = annotated_span.value() {
                    metrics.extend(generic::extract_metrics(span, config, global_options));
                }
            }
        }
    });

    metrics
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use relay_dynamic_config::{Feature, FeatureSet, ProjectConfig};
    use relay_event_normalization::{normalize_event, NormalizationConfig};
    use relay_event_schema::protocol::Timestamp;
    use relay_protocol::Annotated;
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn test_extract_span_metrics_all_modules() {
        let json = r#"
        {
            "type": "transaction",
            "platform": "javascript",
            "start_timestamp": "2021-04-26T07:59:01+0100",
            "timestamp": "2021-04-26T08:00:00+0100",
            "received": "2021-04-26T08:00:01+0100",
            "server_name": "myhost",
            "release": "1.2.3",
            "dist": "foo ",
            "environment": "fake_environment",
            "transaction": "gEt /api/:version/users/",
            "transaction_info": {"source": "custom"},
            "user": {
                "id": "user123",
                "geo": {
                    "country_code": "US"
                }
            },
            "tags": {
                "http.status_code": 500
            },
            "contexts": {
                "trace": {
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "span_id": "bd429c44b67a3eb4",
                    "op": "mYOp",
                    "status": "ok"
                }
            },
            "request": {
                "method": "POST"
            },
            "spans": [
                {
                    "description": "<SomeUiRendering>",
                    "op": "UI.React.Render",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd429c44b67a3eb4",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81"
                },
                {
                    "description": "GET http://domain.tld/hi",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd429c44b67a3eb4",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "data": {
                        "http.method": "GET"
                    }
                },
                {
                    "description": "POST http://domain.tld/hi",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd429c44b67a3eb4",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "data": {
                        "http.request.method": "POST"
                    }
                },
                {
                    "description": "PUT http://domain.tld/hi",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd429c44b67a3eb4",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "data": {
                        "method": "PUT"
                    }
                },
                {
                    "description": "GET /hi/this/is/just/the/path",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd429c44b67a3eb4",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "data": {
                        "http.method": "GET"
                    }
                },
                {
                    "description": "POST http://127.0.0.1:1234/api/hi",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd2eb23da2beb459",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "http.method": "PoSt",
                        "status_code": "200"
                    }
                },
                {
                    "description": "POST http://sth.subdomain.domain.tld:1234/api/hi",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd2eb23da2beb459",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "http.method": "PoSt",
                        "status_code": "200"
                    }
                },
                {
                    "description": "POST http://targetdomain.tld:1234/api/hi",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd2eb23da2beb459",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "http.method": "POST",
                        "status_code": "200"
                    }
                },
                {
                    "description": "POST http://targetdomain:1234/api/id/0987654321",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd2eb23da2beb459",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "http.method": "POST",
                        "status_code": "200"
                    }
                },
                {
                    "description": "POST http://sth.subdomain.domain.tld:1234/api/hi",
                    "op": "http.client",
                    "tags": {
                        "http.status_code": "200"
                    },
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd2eb23da2beb459",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "http.method": "POST"
                    }
                },
                {
                    "description": "POST http://sth.subdomain.domain.tld:1234/api/hi",
                    "op": "http.client",
                    "tags": {
                        "http.status_code": "200"
                    },
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd2eb23da2beb459",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "http.method": "POST",
                        "status_code": "200"
                    }
                },
                {
                    "description": "SeLeCt column FROM tAbLe WHERE id IN (1, 2, 3)",
                    "op": "db.sql.query",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "postgresql",
                        "db.operation": "SELECT"
                    }
                },
                {
                    "description": "select column FROM table WHERE id IN (1, 2, 3)",
                    "op": "db",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok"
                },
                {
                    "description": "INSERT INTO table (col) VALUES (val)",
                    "op": "db.sql.query",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "postgresql",
                        "db.operation": "INSERT"
                    }
                },
                {
                    "description": "INSERT INTO from_date (col) VALUES (val)",
                    "op": "db.sql.query",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "postgresql",
                        "db.operation": "INSERT"
                    }
                },
                {
                    "description": "INSERT INTO table (col) VALUES (val)",
                    "op": "db",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok"
                },
                {
                    "description": "SELECT\n*\nFROM\ntable\nWHERE\nid\nIN\n(val)",
                    "op": "db.sql.query",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "postgresql",
                        "db.operation": "SELECT"
                    }
                },
                {
                    "description": "SELECT \"table\".\"col\" FROM \"table\" WHERE \"table\".\"col\" = %s",
                    "op": "db",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "postgresql",
                        "db.operation": "SELECT"
                    }
                },
                {
                    "description": "DELETE FROM table WHERE conditions",
                    "op": "db",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "MyDatabase"
                    }
                },
                {
                    "description": "UPDATE table WHERE conditions",
                    "op": "db",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "MyDatabase"
                    }
                },
                {
                    "description": "SAVEPOINT save_this_one",
                    "op": "db",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "MyDatabase"
                    }
                },
                {
                    "description": "GET cache:user:{123}",
                    "op": "cache.get_item",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "cache.hit": false
                    }
                },
                {
                    "description": "GET test:123:def",
                    "op": "db.redis",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {}
                },
                {
                    "description": "GET lkjasdlkasjdlasjdlkasjdlkasjd",
                    "op": "db.redis",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {}
                },
                {
                    "description": "SET 'aaa:bbb:123:zzz' '{\"from json\": \"no\"}'",
                    "op": "db.redis",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {}
                },
                {
                    "description": "chrome-extension://begnopegbbhjeeiganiajffnalhlkkjb/img/assets/icon-10k.svg",
                    "op": "resource.script",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok"
                },
                {
                    "description": "http://domain/static/myscript-v1.9.23.js",
                    "op": "resource.script",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok"
                },
                {
                    "timestamp": 1694732408.3145,
                    "start_timestamp": 1694732407.8367,
                    "exclusive_time": 477.800131,
                    "description": "https://cdn.domain.com/path/to/file-hk2YHeW7Eo2XLCiE38F1Fz22KuljsgCAD6hyWCyOYZM.CSS",
                    "op": "resource.css",
                    "span_id": "97c0ef9770a02f9d",
                    "parent_span_id": "9756d8d7b2b364ff",
                    "trace_id": "77aeb1c16bb544a4a39b8d42944947a3",
                    "data": {
                        "http.decoded_response_content_length": 128950,
                        "http.response_content_length": 36170,
                        "http.response_transfer_size": 36470,
                        "resource.render_blocking_status": "blocking"
                    },
                    "hash": "e2fae740cccd3789"
                },
                {
                    "timestamp": 1694732408.3145,
                    "start_timestamp": 1694732407.8367,
                    "span_id": "97c0ef9770a02f9d",
                    "parent_span_id": "9756d8d7b2b364ff",
                    "trace_id": "77aeb1c16bb544a4a39b8d42944947a3",
                    "op": "resource.script",
                    "description": "domain.com/zero-length-00",
                    "data": {
                        "http.decoded_response_content_length": 0,
                        "http.response_content_length": 0,
                        "http.response_transfer_size": 0
                    }
                },
                {
                    "timestamp": 1702474613.0495,
                    "start_timestamp": 1702474613.0175,
                    "description": "input.app-123.adfasf456[type=\"range\"][name=\"replay-timeline\"]",
                    "op": "ui.interaction.click",
                    "span_id": "9b01bd820a083e63",
                    "parent_span_id": "a1e13f3f06239d69",
                    "trace_id": "922dda2462ea4ac2b6a4b339bee90863",
                    "data": {
                        "ui.component_name": "my-component-name"
                    }
                }
            ]
        }
        "#;

        let mut event = Annotated::from_json(json).unwrap();
        let features = FeatureSet(BTreeSet::from([Feature::SpanMetricsExtraction]));

        normalize_event(
            &mut event,
            &NormalizationConfig {
                enrich_spans: true,
                normalize_spans: true,
                ..Default::default()
            },
        );

        // Create a project config with the relevant feature flag. Sanitize to fill defaults.
        let mut project = ProjectConfig {
            features,
            ..ProjectConfig::default()
        };
        project.sanitize();

        let config = project.metric_extraction.ok().unwrap();
        let metrics = extract_metrics(event.value().unwrap(), &config, None);
        insta::assert_debug_snapshot!(metrics);
    }

    #[test]
    fn test_extract_span_metrics() {
        let json = r#"
        {
            "type": "transaction",
            "platform": "javascript",
            "start_timestamp": "2021-04-26T07:59:01+0100",
            "timestamp": "2021-04-26T08:00:00+0100",
            "server_name": "myhost",
            "release": "1.2.3",
            "dist": "foo ",
            "environment": "fake_environment",
            "transaction": "gEt /api/:version/users/",
            "transaction_info": {"source": "custom"},
            "user": {
                "id": "user123",
                "geo": {
                    "country_code": "US"
                }
            },
            "tags": {
                "http.status_code": 500
            },
            "contexts": {
                "trace": {
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "span_id": "bd429c44b67a3eb4",
                    "op": "mYOp",
                    "status": "ok"
                }
            },
            "request": {
                "method": "POST"
            },
            "spans": [
                {
                    "description": "<SomeUiRendering>",
                    "op": "UI.React.Render",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd429c44b67a3eb4",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81"
                },
                {
                    "description": "GET http://domain.tld/hi",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd429c44b67a3eb4",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "data": {
                        "http.method": "GET"
                    }
                },
                {
                    "description": "POST http://domain.tld/hi",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd429c44b67a3eb4",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "data": {
                        "http.request.method": "POST"
                    }
                },
                {
                    "description": "PUT http://domain.tld/hi",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd429c44b67a3eb4",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "data": {
                        "method": "PUT"
                    }
                },
                {
                    "description": "GET /hi/this/is/just/the/path",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd429c44b67a3eb4",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "data": {
                        "http.method": "GET"
                    }
                },
                {
                    "description": "POST http://127.0.0.1:1234/api/hi",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd2eb23da2beb459",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "http.method": "PoSt",
                        "status_code": "200"
                    }
                },
                {
                    "description": "POST http://sth.subdomain.domain.tld:1234/api/hi",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd2eb23da2beb459",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "http.method": "PoSt",
                        "status_code": "200"
                    }
                },
                {
                    "description": "POST http://targetdomain.tld:1234/api/hi",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd2eb23da2beb459",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "http.method": "POST",
                        "status_code": "200"
                    }
                },
                {
                    "description": "POST http://targetdomain:1234/api/id/0987654321",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd2eb23da2beb459",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "http.method": "POST",
                        "status_code": "200"
                    }
                },
                {
                    "description": "POST http://sth.subdomain.domain.tld:1234/api/hi",
                    "op": "http.client",
                    "tags": {
                        "http.status_code": "200"
                    },
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd2eb23da2beb459",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "http.method": "POST"
                    }
                },
                {
                    "description": "POST http://sth.subdomain.domain.tld:1234/api/hi",
                    "op": "http.client",
                    "tags": {
                        "http.status_code": "200"
                    },
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd2eb23da2beb459",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "http.method": "POST",
                        "status_code": "200"
                    }
                },
                {
                    "description": "SeLeCt column FROM tAbLe WHERE id IN (1, 2, 3)",
                    "op": "db.sql.query",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "postgresql",
                        "db.operation": "SELECT"
                    }
                },
                {
                    "description": "select column FROM table WHERE id IN (1, 2, 3)",
                    "op": "db",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok"
                },
                {
                    "description": "INSERT INTO table (col) VALUES (val)",
                    "op": "db.sql.query",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "postgresql",
                        "db.operation": "INSERT"
                    }
                },
                {
                    "description": "INSERT INTO from_date (col) VALUES (val)",
                    "op": "db.sql.query",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "postgresql",
                        "db.operation": "INSERT"
                    }
                },
                {
                    "description": "INSERT INTO table (col) VALUES (val)",
                    "op": "db",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok"
                },
                {
                    "description": "SELECT\n*\nFROM\ntable\nWHERE\nid\nIN\n(val)",
                    "op": "db.sql.query",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "postgresql",
                        "db.operation": "SELECT"
                    }
                },
                {
                    "description": "SELECT \"table\".\"col\" FROM \"table\" WHERE \"table\".\"col\" = %s",
                    "op": "db",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "postgresql",
                        "db.operation": "SELECT"
                    }
                },
                {
                    "description": "DELETE FROM table WHERE conditions",
                    "op": "db",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "MyDatabase"
                    }
                },
                {
                    "description": "UPDATE table WHERE conditions",
                    "op": "db",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "MyDatabase"
                    }
                },
                {
                    "description": "SAVEPOINT save_this_one",
                    "op": "db",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "MyDatabase"
                    }
                },
                {
                    "description": "GET cache:user:{123}",
                    "op": "cache.get_item",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "cache.hit": false
                    }
                },
                {
                    "description": "GET test:123:def",
                    "op": "db.redis",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {}
                },
                {
                    "description": "GET lkjasdlkasjdlasjdlkasjdlkasjd",
                    "op": "db.redis",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {}
                },
                {
                    "description": "SET 'aaa:bbb:123:zzz' '{\"from json\": \"no\"}'",
                    "op": "db.redis",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {}
                },
                {
                    "description": "http://domain/static/myscript-v1.9.23.js",
                    "op": "resource.script",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok"
                },
                {
                    "description": "things.count({\"$and\":[{\"services\":{\"$exists\":true}},{\"test_id\":38}]})",
                    "op": "db.sql.query",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "mongodb",
                        "db.operation": "count"
                    }
                },
                {
                    "description": "DELETE FROM table WHERE conditions",
                    "op": "db.sql.query",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "MyDatabase"
                    }
                },
                {
                    "description": "things.count({\"$and\":[{\"services\":{\"$exists\":true}},{\"test_id\":38}]})",
                    "op": "db.mongodb.find",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {}
                },
                {
                    "description": "DELETE FROM table WHERE conditions",
                    "op": "db.sql.activerecord",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "MyDatabase"
                    }
                },
                {
                    "description": "SAVEPOINT save_this_one",
                    "op": "db.redis.command",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bb7af8b99e95af5f",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "status": "ok",
                    "data": {
                        "db.system": "redis"
                    }
                },
                {
                    "data": {
                        "device.class": "2",
                        "environment": "production",
                        "http.response.status_code": "200",
                        "mobile": true,
                        "release": "sentrydemos.ios.EmpowerPlant@0.0.8+1",
                        "span.op": "ui.load",
                        "span.status": "ok",
                        "transaction": "EmpowerPlantViewController",
                        "transaction.op": "ui.load",
                        "user": "id:FADC011D-28AA-40B7-8CA8-839A2AD05168"
                    },
                    "description": "viewDidLoad",
                    "exclusive_time": 15101.696732,
                    "op": "ui.load",
                    "parent_span_id": "6ebd1cdbb9424b88",
                    "span_id": "8cfaf7f29ac345b8",
                    "start_timestamp": 1695255136.239635,
                    "status": "ok",
                    "timestamp": 1695255152.073167,
                    "trace_id": "2dc90ee797b94299ba5ad82b816fc9f8"
                },
                {
                    "data": {
                        "device.class": "2",
                        "environment": "production",
                        "http.response.status_code": "200",
                        "mobile": true,
                        "release": "sentrydemos.ios.EmpowerPlant@0.0.8+1",
                        "span.op": "app.start.cold",
                        "span.status": "ok",
                        "transaction": "EmpowerPlantViewController",
                        "transaction.op": "ui.load",
                        "user": "id:FADC011D-28AA-40B7-8CA8-839A2AD05168"
                    },
                    "description": "Cold Start",
                    "exclusive_time": 0.0,
                    "op": "app.start.cold",
                    "parent_span_id": "6ebd1cdbb9424b88",
                    "span_id": "0e989cd370034c76",
                    "start_timestamp": 1695255134.469436,
                    "timestamp": 1695255136.137952,
                    "trace_id": "2dc90ee797b94299ba5ad82b816fc9f8"
                },
                {
                    "timestamp": 1694732408.3145,
                    "start_timestamp": 1694732407.8367,
                    "exclusive_time": 477.800131,
                    "description": "https://cdn.domain.com/path/to/file-hk2YHeW7Eo2XLCiE38F1Fz22KuljsgCAD6hyWCyOYZM.css",
                    "op": "resource.css",
                    "span_id": "97c0ef9770a02f9d",
                    "parent_span_id": "9756d8d7b2b364ff",
                    "trace_id": "77aeb1c16bb544a4a39b8d42944947a3",
                    "data": {
                        "http.decoded_response_content_length": 128950,
                        "http.response_content_length": 36170,
                        "http.response_transfer_size": 36470,
                        "resource.render_blocking_status": "blocking"
                    },
                    "hash": "e2fae740cccd3789"
                },
                {
                    "timestamp": 1694732408.3145,
                    "start_timestamp": 1694732407.8367,
                    "exclusive_time": 477.800131,
                    "description": "/static/myscript-v1.9.23.js",
                    "op": "resource.script",
                    "span_id": "97c0ef9770a02f9d",
                    "parent_span_id": "9756d8d7b2b364ff",
                    "trace_id": "77aeb1c16bb544a4a39b8d42944947a3",
                    "data": {
                        "http.decoded_response_content_length": 128950,
                        "http.response_content_length": 36170,
                        "http.response_transfer_size": 36470,
                        "resource.render_blocking_status": "blocking",
                        "server.address": "subdomain.example.com:5688",
                        "url.same_origin": true,
                        "url.scheme": "https"
                    },
                    "hash": "e2fae740cccd3789"
                }
            ]
        }
        "#;

        let mut event = Annotated::from_json(json).unwrap();
        let features = FeatureSet(BTreeSet::from([Feature::SpanMetricsExtraction]));

        // Normalize first, to make sure that all things are correct as in the real pipeline:
        normalize_event(
            &mut event,
            &NormalizationConfig {
                enrich_spans: true,
                normalize_spans: true,
                ..Default::default()
            },
        );

        // Create a project config with the relevant feature flag. Sanitize to fill defaults.
        let mut project = ProjectConfig {
            features,
            ..ProjectConfig::default()
        };
        project.sanitize();

        let config = project.metric_extraction.ok().unwrap();
        let metrics = extract_metrics(event.value().unwrap(), &config, None);
        insta::assert_debug_snapshot!(metrics);
    }

    const MOBILE_EVENT: &str = r#"
        {
            "type": "transaction",
            "sdk": {"name": "sentry.javascript.react-native"},
            "start_timestamp": "2021-04-26T07:59:01+0100",
            "timestamp": "2021-04-26T08:00:00+0100",
            "release": "1.2.3",
            "transaction": "gEt /api/:version/users/",
            "transaction_info": {"source": "custom"},
            "contexts": {
                "trace": {
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "span_id": "bd429c44b67a3eb4"
                },
                "device": {
                    "family": "iOS",
                    "model": "iPhone1,1"
                },
                "app": {
                    "app_identifier": "org.reactjs.native.example.RnDiffApp",
                    "app_name": "RnDiffApp"
                },
                "os": {
                    "name": "iOS",
                    "version": "16.2"
                }
            },
            "spans": [
                {
                    "op": "app.start.cold",
                    "span_id": "bd429c44b67a3eb4",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81"
                },
                {
                    "op": "ui.load.initial_display",
                    "span_id": "bd429c44b67a3eb2",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976303.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81"
                },
                {
                    "op": "app.start.cold",
                    "description": "Cold Start",
                    "span_id": "bd429c44b67a3eb2",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976303.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81"
                },
                {
                    "op": "custom.op",
                    "description": "Custom Op",
                    "span_id": "bd429c44b67a3eb2",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976303.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81"
                },
                {
                    "op": "contentprovider.load",
                    "description": "io.sentry.android.core.SentryPerformanceProvider.onCreate",
                    "span_id": "bd429c44b67a3eb2",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976303.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81"
                },
                {
                    "op": "application.load",
                    "description": "io.sentry.samples.android.MyApplication.onCreate",
                    "span_id": "bd429c44b67a3eb2",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976303.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81"
                },
                {
                    "op": "activity.load",
                    "description": "io.sentry.samples.android.MainActivity.onCreate",
                    "span_id": "bd429c44b67a3eb2",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976303.0000000,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81"
                }
            ]
        }
        "#;

    #[test]
    fn test_extract_span_metrics_mobile() {
        let mut event = Annotated::from_json(MOBILE_EVENT).unwrap();

        // Normalize first, to make sure that all things are correct as in the real pipeline:
        normalize_event(
            &mut event,
            &NormalizationConfig {
                enrich_spans: true,
                normalize_spans: true,
                device_class_synthesis_config: true,
                ..Default::default()
            },
        );

        // Create a project config with the relevant feature flag. Sanitize to fill defaults.
        let mut project = ProjectConfig {
            features: [Feature::SpanMetricsExtraction].into_iter().collect(),
            ..ProjectConfig::default()
        };
        project.sanitize();

        let config = project.metric_extraction.ok().unwrap();
        let metrics = extract_metrics(event.value().unwrap(), &config, None);
        insta::assert_debug_snapshot!((&event.value().unwrap().spans, metrics));
    }

    #[test]
    fn test_extract_span_metrics_mobile_screen() {
        let json = r#"
        {
            "type": "transaction",
            "sdk": {"name": "sentry.javascript.react-native"},
            "start_timestamp": "2021-04-26T07:59:01+0100",
            "timestamp": "2021-04-26T08:00:00+0100",
            "transaction": "gEt /api/:version/users/",
            "contexts": {
                "trace": {
                    "op": "ui.load",
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "span_id": "bd429c44b67a3eb4"
                }
            },
            "spans": [
                {
                    "description": "GET http://domain.tld/hi",
                    "op": "http.client",
                    "parent_span_id": "8f5a2b8768cafb4e",
                    "span_id": "bd429c44b67a3eb4",
                    "start_timestamp": 1597976300.0000000,
                    "timestamp": 1597976302.0000000,
                    "exclusive_time": 2000.0,
                    "trace_id": "ff62a8b040f340bda5d830223def1d81",
                    "data": {
                        "http.method": "GET"
                    },
                    "sentry_tags": {
                        "action": "GET",
                        "category": "http",
                        "description": "GET http://domain.tld",
                        "domain": "domain.tld",
                        "group": "d9dc18637d441612",
                        "mobile": "true",
                        "op": "http.client",
                        "transaction": "gEt /api/:version/users/",
                        "transaction.method": "GET",
                        "transaction.op": "ui.load"
                    }
                }
            ]
        }
        "#;
        let event = Annotated::from_json(json).unwrap();

        // Create a project config with the relevant feature flag. Sanitize to fill defaults.
        let mut project = ProjectConfig {
            features: [Feature::SpanMetricsExtraction].into_iter().collect(),
            ..ProjectConfig::default()
        };
        project.sanitize();

        let config = project.metric_extraction.ok().unwrap();
        let metrics = extract_metrics(event.value().unwrap(), &config, None);

        // When transaction.op:ui.load and mobile:true, HTTP spans still get both
        // exclusive_time metrics:
        assert!(metrics
            .iter()
            .any(|b| b.name == "d:spans/exclusive_time@millisecond"));
        assert!(metrics
            .iter()
            .any(|b| b.name == "d:spans/exclusive_time_light@millisecond"));
    }

    #[test]
    fn test_extract_span_metrics_usage() {
        let mut event = Annotated::from_json(MOBILE_EVENT).unwrap();

        // Normalize first, to make sure that all things are correct as in the real pipeline:
        normalize_event(
            &mut event,
            &NormalizationConfig {
                enrich_spans: true,
                normalize_spans: true,
                device_class_synthesis_config: true,
                ..Default::default()
            },
        );

        // Create a project config with the relevant feature flag. Sanitize to fill defaults.
        let mut project = ProjectConfig {
            features: [Feature::SpanMetricsExtraction].into_iter().collect(),
            ..ProjectConfig::default()
        };
        project.sanitize();

        let config = project.metric_extraction.ok().unwrap();
        let metrics = extract_metrics(
            event.value().unwrap(),
            &config,
            Some(&{
                let mut o = Options::default();
                o.span_usage_metric = true;
                o
            }),
        );

        let usage_metrics = metrics
            .into_iter()
            .filter(|b| b.name == "c:spans/usage@none")
            .collect::<Vec<_>>();

        let expected_usage = 6; // There are 7 spans, but `custom.op` is not counted.
        assert_eq!(usage_metrics.len(), expected_usage);
        for m in usage_metrics {
            assert!(m.tags.is_empty());
        }
    }

    /// Helper function for span metric extraction tests.
    fn extract_span_metrics(span: &Span) -> Vec<Bucket> {
        let mut config = ProjectConfig::default();
        config.features.0.insert(Feature::SpanMetricsExtraction);
        config.sanitize(); // apply defaults for span extraction

        let extraction_config = config.metric_extraction.ok().unwrap();
        generic::extract_metrics(span, &extraction_config, None)
    }

    /// Helper function for span metric extraction tests.
    fn extract_span_metrics_mobile(span_op: &str, duration_millis: f64) -> Vec<Bucket> {
        let mut span = Span::default();
        span.sentry_tags
            .get_or_insert_with(Default::default)
            .insert("mobile".to_owned(), "true".to_owned().into());
        span.timestamp
            .set_value(Some(Timestamp::from(DateTime::<Utc>::MAX_UTC))); // whatever
        span.op.set_value(Some(span_op.into()));
        span.exclusive_time.set_value(Some(duration_millis));

        extract_span_metrics(&span)
    }

    #[test]
    fn test_app_start_cold_inlier() {
        let metrics = extract_span_metrics_mobile("app.start.cold", 180000.0);
        assert_eq!(
            metrics.iter().map(|m| &m.name).collect::<Vec<_>>(),
            vec![
                "d:spans/exclusive_time@millisecond",
                "d:spans/exclusive_time_light@millisecond",
                "c:spans/count_per_op@none",
                "c:spans/count_per_segment@none"
            ]
        );
    }

    #[test]
    fn test_app_start_cold_outlier() {
        let metrics = extract_span_metrics_mobile("app.start.cold", 181000.0);
        assert!(metrics.is_empty());
    }

    #[test]
    fn test_app_start_warm_inlier() {
        let metrics = extract_span_metrics_mobile("app.start.warm", 180000.0);
        assert_eq!(
            metrics.iter().map(|m| &m.name).collect::<Vec<_>>(),
            vec![
                "d:spans/exclusive_time@millisecond",
                "d:spans/exclusive_time_light@millisecond",
                "c:spans/count_per_op@none",
                "c:spans/count_per_segment@none"
            ]
        );
    }

    #[test]
    fn test_app_start_warm_outlier() {
        let metrics = extract_span_metrics_mobile("app.start.warm", 181000.0);
        assert!(metrics.is_empty());
    }

    #[test]
    fn test_ui_load_initial_display_inlier() {
        let metrics = extract_span_metrics_mobile("ui.load.initial_display", 180000.0);
        assert_eq!(
            metrics.iter().map(|m| &m.name).collect::<Vec<_>>(),
            vec![
                "d:spans/exclusive_time@millisecond",
                "d:spans/exclusive_time_light@millisecond",
                "c:spans/count_per_op@none",
                "c:spans/count_per_segment@none"
            ]
        );
    }

    #[test]
    fn test_ui_load_initial_display_outlier() {
        let metrics = extract_span_metrics_mobile("ui.load.initial_display", 181000.0);
        assert!(metrics.is_empty());
    }

    #[test]
    fn test_ui_load_full_display_inlier() {
        let metrics = extract_span_metrics_mobile("ui.load.full_display", 180000.0);
        assert_eq!(
            metrics.iter().map(|m| &m.name).collect::<Vec<_>>(),
            vec![
                "d:spans/exclusive_time@millisecond",
                "d:spans/exclusive_time_light@millisecond",
                "c:spans/count_per_op@none",
                "c:spans/count_per_segment@none"
            ]
        );
    }

    #[test]
    fn test_ui_load_full_display_outlier() {
        let metrics = extract_span_metrics_mobile("ui.load.full_display", 181000.0);
        assert!(metrics.is_empty());
    }

    #[test]
    fn test_display_times_extracted() {
        let span = r#"{
            "op": "ui.load",
            "span_id": "bd429c44b67a3eb4",
            "start_timestamp": 1597976300.0000000,
            "timestamp": 1597976302.0000000,
            "exclusive_time": 100,
            "trace_id": "ff62a8b040f340bda5d830223def1d81",
            "sentry_tags": {
                "mobile": "true",
                "ttid": "ttid",
                "ttfd": "ttfd"
            }
        }"#;
        let span = Annotated::from_json(span).unwrap().into_value().unwrap();
        let metrics = extract_span_metrics(&span);

        assert!(!metrics.is_empty());
        for metric in metrics {
            if metric.name == "c:spans/count_per_op@none"
                || metric.name == "c:spans/count_per_segment@none"
            {
                continue;
            }
            if metric.name == "d:spans/exclusive_time_light@millisecond" {
                assert!(!metric.tags.contains_key("ttid"));
                assert!(!metric.tags.contains_key("ttfd"));
                continue;
            }
            assert_eq!(metric.tag("ttid"), Some("ttid"));
            assert_eq!(metric.tag("ttfd"), Some("ttfd"));
        }
    }
}
