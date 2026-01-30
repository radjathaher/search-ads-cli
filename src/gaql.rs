use anyhow::Result;
use prost_reflect::DescriptorPool;
use serde_json::{Map, Value, json};

use crate::client::AdsClient;
use crate::command_tree::find_method;
use crate::proto_json::{dynamic_from_value, dynamic_to_value};

pub struct SearchArgs {
    pub customer_id: String,
    pub query: String,
    pub use_search: bool,
    pub page_size: Option<i64>,
    pub page_token: Option<String>,
    pub validate_only: bool,
    pub summary_row_setting: Option<String>,
    pub return_total_results_count: bool,
    pub raw: bool,
    pub jsonl: bool,
}

pub enum Output {
    Json(Value),
    JsonLines(Vec<Value>),
}

const SERVICE: &str = "google-ads-service";
const SEARCH: &str = "search";
const SEARCH_STREAM: &str = "search-stream";

pub async fn run_search(client: &AdsClient, pool: &DescriptorPool, args: SearchArgs) -> Result<Output> {
    if args.use_search {
        let method = find_method(pool, SERVICE, SEARCH)?;
        let request = build_search_request(&args);
        let message = dynamic_from_value(method.input(), request)?;
        let response = client.unary(&method, message).await?;
        let json = dynamic_to_value(&response)?;
        if args.raw {
            return Ok(Output::Json(json));
        }
        let results = json
            .get("results")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        return Ok(Output::Json(results));
    }

    let method = find_method(pool, SERVICE, SEARCH_STREAM)?;
    let request = build_search_request(&args);
    let message = dynamic_from_value(method.input(), request)?;

    if args.jsonl {
        let mut stream = client.server_streaming_raw(&method, message).await?;
        let mut rows = Vec::new();
        while let Some(msg) = stream.message().await? {
            let json = dynamic_to_value(&msg)?;
            if args.raw {
                rows.push(json);
                continue;
            }
            if let Some(results) = json.get("results").and_then(|v| v.as_array()) {
                for row in results {
                    rows.push(row.clone());
                }
            }
        }
        return Ok(Output::JsonLines(rows));
    }

    let responses = client.server_stream(&method, message).await?;
    let mut chunks = Vec::new();
    let mut rows = Vec::new();
    for msg in responses {
        let json = dynamic_to_value(&msg)?;
        if args.raw {
            chunks.push(json);
            continue;
        }
        if let Some(results) = json.get("results").and_then(|v| v.as_array()) {
            for row in results {
                rows.push(row.clone());
            }
        }
    }

    if args.raw {
        return Ok(Output::Json(Value::Array(chunks)));
    }
    Ok(Output::Json(Value::Array(rows)))
}

fn build_search_request(args: &SearchArgs) -> Value {
    let mut map = Map::new();
    map.insert("customerId".to_string(), Value::String(args.customer_id.clone()));
    map.insert("query".to_string(), Value::String(args.query.clone()));

    if args.use_search {
        if let Some(page_size) = args.page_size {
            map.insert("pageSize".to_string(), json!(page_size));
        }
        if let Some(page_token) = args.page_token.as_ref() {
            map.insert("pageToken".to_string(), Value::String(page_token.clone()));
        }
    }
    if args.validate_only {
        map.insert("validateOnly".to_string(), Value::Bool(true));
    }
    if let Some(setting) = args.summary_row_setting.as_ref() {
        map.insert("summaryRowSetting".to_string(), Value::String(setting.clone()));
    }
    if args.use_search && args.return_total_results_count {
        map.insert("returnTotalResultsCount".to_string(), Value::Bool(true));
    }

    Value::Object(map)
}
