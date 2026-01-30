use anyhow::{Context, Result};
use prost_reflect::{DynamicMessage, MessageDescriptor};
use serde_json::Value;

pub fn dynamic_from_value(desc: MessageDescriptor, value: Value) -> Result<DynamicMessage> {
    let json = serde_json::to_string(&value).context("serialize json")?;
    let mut de = serde_json::Deserializer::from_str(&json);
    let msg = DynamicMessage::deserialize(desc, &mut de).context("decode json into proto")?;
    de.end().context("extra json input")?;
    Ok(msg)
}

pub fn dynamic_to_value(message: &DynamicMessage) -> Result<Value> {
    serde_json::to_value(message).context("encode proto json")
}
