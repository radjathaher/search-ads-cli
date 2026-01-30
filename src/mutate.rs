use anyhow::{Result, anyhow};
use prost_reflect::DescriptorPool;
use serde_json::{Map, Value};

use crate::client::AdsClient;
use crate::command_tree::find_method;
use crate::proto_json::{dynamic_from_value, dynamic_to_value};

pub struct MutateArgs {
    pub customer_id: String,
    pub ops: Option<Value>,
    pub body: Option<Value>,
    pub partial_failure: bool,
    pub validate_only: bool,
    pub response_content_type: Option<String>,
}

const SERVICE: &str = "google-ads-service";
const MUTATE: &str = "mutate";

pub async fn run_mutate(client: &AdsClient, pool: &DescriptorPool, args: MutateArgs) -> Result<Value> {
    let method = find_method(pool, SERVICE, MUTATE)?;
    let body = if let Some(body) = args.body {
        body
    } else {
        build_request(&args)?
    };

    let message = dynamic_from_value(method.input(), body)?;
    let response = client.unary(&method, message).await?;
    dynamic_to_value(&response)
}

fn build_request(args: &MutateArgs) -> Result<Value> {
    let mut map = Map::new();
    map.insert("customerId".to_string(), Value::String(args.customer_id.clone()));

    let ops = args
        .ops
        .clone()
        .ok_or_else(|| anyhow!("--ops required unless --body provided"))?;
    map.insert("mutateOperations".to_string(), ops);

    if args.partial_failure {
        map.insert("partialFailure".to_string(), Value::Bool(true));
    }
    if args.validate_only {
        map.insert("validateOnly".to_string(), Value::Bool(true));
    }
    if let Some(response_content_type) = args.response_content_type.as_ref() {
        map.insert(
            "responseContentType".to_string(),
            Value::String(response_content_type.clone()),
        );
    }

    Ok(Value::Object(map))
}
