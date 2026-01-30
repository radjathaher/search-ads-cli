use anyhow::{Result, anyhow};
use prost_reflect::{Cardinality, DescriptorPool, FieldDescriptor, Kind, MethodDescriptor, ServiceDescriptor};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CommandTree {
    pub version: u32,
    pub api_version: String,
    pub services: Vec<ServiceDef>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServiceDef {
    pub name: String,
    pub full_name: String,
    pub methods: Vec<MethodDef>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MethodDef {
    pub name: String,
    pub full_name: String,
    pub input_type: String,
    pub output_type: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MethodDescription {
    pub service: String,
    pub method: String,
    pub input_type: String,
    pub output_type: String,
    pub fields: Vec<FieldDef>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FieldDef {
    pub name: String,
    pub json_name: String,
    pub cardinality: String,
    pub kind: String,
    pub type_name: Option<String>,
}

const DESCRIPTOR_BYTES: &[u8] = include_bytes!("../schemas/googleads.desc");
const GOOGLE_ADS_PREFIX: &str = "google.ads.googleads.";
const SERVICES_SEGMENT: &str = ".services.";

pub fn load_pool() -> DescriptorPool {
    DescriptorPool::decode(DESCRIPTOR_BYTES).expect("invalid googleads.desc")
}

pub fn build_tree(pool: &DescriptorPool) -> CommandTree {
    let mut services = Vec::new();

    for service in pool.services() {
        if !is_google_ads_service(&service) {
            continue;
        }

        let mut methods = Vec::new();
        for method in service.methods() {
            methods.push(MethodDef {
                name: to_kebab(method.name()),
                full_name: format!("{}/{}", service.full_name(), method.name()),
                input_type: method.input().full_name().to_string(),
                output_type: method.output().full_name().to_string(),
                client_streaming: method.is_client_streaming(),
                server_streaming: method.is_server_streaming(),
            });
        }
        methods.sort_by(|a, b| a.name.cmp(&b.name));

        services.push(ServiceDef {
            name: to_kebab(service.name()),
            full_name: service.full_name().to_string(),
            methods,
        });
    }

    services.sort_by(|a, b| a.name.cmp(&b.name));

    CommandTree {
        version: 1,
        api_version: detect_api_version(pool).unwrap_or_else(|| "unknown".to_string()),
        services,
    }
}

pub fn describe_method(pool: &DescriptorPool, service: &str, method: &str) -> Result<MethodDescription> {
    let method_desc = find_method(pool, service, method)?;
    let input = method_desc.input();
    let output = method_desc.output();

    let fields = input
        .fields()
        .map(field_def)
        .collect::<Vec<_>>();

    Ok(MethodDescription {
        service: method_desc.parent_service().full_name().to_string(),
        method: method_desc.name().to_string(),
        input_type: input.full_name().to_string(),
        output_type: output.full_name().to_string(),
        fields,
    })
}

pub fn find_method(pool: &DescriptorPool, service: &str, method: &str) -> Result<MethodDescriptor> {
    let target_service = find_service(pool, service)?;
    let target_method = target_service
        .methods()
        .find(|m| names_match(m.name(), method) || names_match(&to_kebab(m.name()), method))
        .ok_or_else(|| anyhow!("unknown method {service} {method}"))?;
    Ok(target_method)
}

fn find_service(pool: &DescriptorPool, service: &str) -> Result<ServiceDescriptor> {
    pool.services()
        .find(|s| is_google_ads_service(s) && service_matches(s, service))
        .ok_or_else(|| anyhow!("unknown service {service}"))
}

fn service_matches(service: &ServiceDescriptor, input: &str) -> bool {
    names_match(service.name(), input)
        || names_match(&to_kebab(service.name()), input)
        || names_match(service.full_name(), input)
}

fn names_match(candidate: &str, input: &str) -> bool {
    normalize(candidate) == normalize(input)
}

fn normalize(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        }
    }
    out
}

fn is_google_ads_service(service: &ServiceDescriptor) -> bool {
    service.full_name().starts_with(GOOGLE_ADS_PREFIX) && service.full_name().contains(SERVICES_SEGMENT)
}

fn detect_api_version(pool: &DescriptorPool) -> Option<String> {
    for service in pool.services() {
        if !is_google_ads_service(&service) {
            continue;
        }
        for part in service.full_name().split('.') {
            if part.starts_with('v') && part.len() > 1 && part[1..].chars().all(|c| c.is_ascii_digit()) {
                return Some(part.to_string());
            }
        }
    }
    None
}

fn field_def(field: FieldDescriptor) -> FieldDef {
    let kind = match field.kind() {
        Kind::Message(m) => format!("message:{}", m.full_name()),
        Kind::Enum(e) => format!("enum:{}", e.full_name()),
        other => format!("scalar:{:?}", other).to_lowercase(),
    };

    FieldDef {
        name: field.name().to_string(),
        json_name: field.json_name().to_string(),
        cardinality: match field.cardinality() {
            Cardinality::Optional => "optional".to_string(),
            Cardinality::Required => "required".to_string(),
            Cardinality::Repeated => "repeated".to_string(),
        },
        kind,
        type_name: type_name(&field),
    }
}

fn type_name(field: &FieldDescriptor) -> Option<String> {
    match field.kind() {
        Kind::Message(m) => Some(m.full_name().to_string()),
        Kind::Enum(e) => Some(e.full_name().to_string()),
        other => Some(format!("{:?}", other).to_lowercase()),
    }
}

fn to_kebab(value: &str) -> String {
    let mut out = String::new();
    for (i, ch) in value.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
        } else if ch == '_' {
            out.push('-');
        } else {
            out.push(ch);
        }
    }
    out
}
