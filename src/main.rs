mod auth;
mod client;
mod command_tree;
mod gaql;
mod json_input;
mod mutate;
mod proto_json;

use anyhow::{Result, anyhow};
use clap::{Arg, ArgAction, Command, value_parser};
use serde_json::{Value, json};
use std::env;
use std::io::Write;
use std::time::Duration;

use auth::{AuthConfig, normalize_customer_id};
use client::AdsClient;
use command_tree::{CommandTree, build_tree, describe_method, load_pool, find_method};
use gaql::{SearchArgs, Output as GaqlOutput};
use json_input::read_json_input;
use mutate::MutateArgs;
use proto_json::{dynamic_from_value, dynamic_to_value};

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let pool = load_pool();
    let tree = build_tree(&pool);
    let cli = build_cli(&tree);
    let matches = cli.get_matches();

    if let Some(matches) = matches.subcommand_matches("list") {
        return handle_list(&tree, matches);
    }
    if let Some(matches) = matches.subcommand_matches("describe") {
        return handle_describe(&pool, matches);
    }
    if let Some(matches) = matches.subcommand_matches("tree") {
        return handle_tree(&tree, matches);
    }

    let config = load_config(&matches)?;
    let auth = AuthConfig {
        access_token: config.access_token.clone(),
        client_id: config.client_id.clone(),
        client_secret: config.client_secret.clone(),
        refresh_token: config.refresh_token.clone(),
    };
    let access_token = auth::resolve_access_token(&auth).await?;
    let timeout = matches
        .get_one::<u64>("timeout")
        .copied()
        .map(Duration::from_secs);

    let client = AdsClient::connect(
        &config.endpoint,
        config.developer_token,
        config.login_customer_id,
        access_token,
        timeout,
    )
    .await?;

    let pretty = matches.get_flag("pretty");
    let jsonl = matches.get_flag("jsonl");
    let raw = matches.get_flag("raw");

    if let Some(matches) = matches.subcommand_matches("gaql") {
        let matches = matches
            .subcommand_matches("search")
            .ok_or_else(|| anyhow!("gaql search required"))?;
        let customer_id = read_customer_id(matches)?;
        let query = matches
            .get_one::<String>("query")
            .ok_or_else(|| anyhow!("--query required"))?
            .to_string();
        let args = SearchArgs {
            customer_id,
            query,
            use_search: matches.get_flag("use_search"),
            page_size: matches.get_one::<i64>("page_size").copied(),
            page_token: matches.get_one::<String>("page_token").cloned(),
            validate_only: matches.get_flag("validate_only"),
            summary_row_setting: matches.get_one::<String>("summary_row_setting").cloned(),
            return_total_results_count: matches.get_flag("return_total_results_count"),
            raw,
            jsonl,
        };
        let output = gaql::run_search(&client, &pool, args).await?;
        return write_gaql_output(output, pretty);
    }

    if let Some(matches) = matches.subcommand_matches("mutate") {
        let customer_id = read_customer_id(matches)?;
        let body = matches
            .get_one::<String>("body")
            .map(|v| read_json_input(v))
            .transpose()?;
        let ops = matches
            .get_one::<String>("ops")
            .map(|v| read_json_input(v))
            .transpose()?;
        let args = MutateArgs {
            customer_id,
            ops,
            body,
            partial_failure: matches.get_flag("partial_failure"),
            validate_only: matches.get_flag("validate_only"),
            response_content_type: matches.get_one::<String>("response_content_type").cloned(),
        };
        let output = mutate::run_mutate(&client, &pool, args).await?;
        return write_json(&output, pretty);
    }

    if let Some(matches) = matches.subcommand_matches("raw") {
        let service = matches
            .get_one::<String>("service")
            .ok_or_else(|| anyhow!("--service required"))?;
        let method = matches
            .get_one::<String>("method")
            .ok_or_else(|| anyhow!("--method required"))?;
        let body = matches
            .get_one::<String>("body")
            .ok_or_else(|| anyhow!("--body required"))?;
        let method_desc = find_method(&pool, service, method)?;
        if method_desc.is_client_streaming() {
            return Err(anyhow!("client streaming is not supported"));
        }

        let body_value = read_json_input(body)?;
        let request = dynamic_from_value(method_desc.input(), body_value)?;

        if method_desc.is_server_streaming() {
            if jsonl {
                let mut stream = client.server_streaming_raw(&method_desc, request).await?;
                while let Some(msg) = stream.message().await? {
                    let json = dynamic_to_value(&msg)?;
                    write_stdout_line(&serde_json::to_string(&json)?)?;
                }
                return Ok(());
            }
            let responses = client.server_stream(&method_desc, request).await?;
            let mut values = Vec::new();
            for msg in responses {
                values.push(dynamic_to_value(&msg)?);
            }
            return write_json(&Value::Array(values), pretty);
        }

        let response = client.unary(&method_desc, request).await?;
        let json = dynamic_to_value(&response)?;
        return write_json(&json, pretty);
    }

    Err(anyhow!("command required"))
}

fn build_cli(tree: &CommandTree) -> Command {
    let mut cmd = Command::new("search-ads")
        .about("Google Ads API CLI (gRPC, dynamic)")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .arg(
            Arg::new("pretty")
                .long("pretty")
                .global(true)
                .action(ArgAction::SetTrue)
                .help("Pretty-print JSON output"),
        )
        .arg(
            Arg::new("jsonl")
                .long("jsonl")
                .global(true)
                .action(ArgAction::SetTrue)
                .help("Emit JSON lines for streaming responses"),
        )
        .arg(
            Arg::new("raw")
                .long("raw")
                .global(true)
                .action(ArgAction::SetTrue)
                .help("Return raw response payloads"),
        )
        .arg(
            Arg::new("developer_token")
                .long("developer-token")
                .global(true)
                .value_name("TOKEN")
                .help("Developer token (env: GOOGLE_ADS_DEVELOPER_TOKEN)"),
        )
        .arg(
            Arg::new("access_token")
                .long("access-token")
                .global(true)
                .value_name("TOKEN")
                .help("Access token (env: GOOGLE_ADS_ACCESS_TOKEN)"),
        )
        .arg(
            Arg::new("client_id")
                .long("client-id")
                .global(true)
                .value_name("ID")
                .help("OAuth client id (env: GOOGLE_ADS_CLIENT_ID)"),
        )
        .arg(
            Arg::new("client_secret")
                .long("client-secret")
                .global(true)
                .value_name("SECRET")
                .help("OAuth client secret (env: GOOGLE_ADS_CLIENT_SECRET)"),
        )
        .arg(
            Arg::new("refresh_token")
                .long("refresh-token")
                .global(true)
                .value_name("TOKEN")
                .help("OAuth refresh token (env: GOOGLE_ADS_REFRESH_TOKEN)"),
        )
        .arg(
            Arg::new("login_customer_id")
                .long("login-customer-id")
                .global(true)
                .value_name("ID")
                .help("Manager account id (env: GOOGLE_ADS_LOGIN_CUSTOMER_ID)"),
        )
        .arg(
            Arg::new("endpoint")
                .long("endpoint")
                .global(true)
                .value_name("URL")
                .help("API endpoint (env: GOOGLE_ADS_ENDPOINT)"),
        )
        .arg(
            Arg::new("timeout")
                .long("timeout")
                .global(true)
                .value_parser(value_parser!(u64))
                .help("Request timeout in seconds"),
        )
        .arg(
            Arg::new("debug")
                .long("debug")
                .global(true)
                .action(ArgAction::SetTrue)
                .help("Enable debug logging"),
        );

    cmd = cmd.subcommand(
        Command::new("list")
            .about("List services and methods")
            .arg(
                Arg::new("json")
                    .long("json")
                    .action(ArgAction::SetTrue)
                    .help("Emit machine-readable JSON"),
            ),
    );

    cmd = cmd.subcommand(
        Command::new("describe")
            .about("Describe a service method")
            .arg(Arg::new("service").required(true))
            .arg(Arg::new("method").required(true))
            .arg(
                Arg::new("json")
                    .long("json")
                    .action(ArgAction::SetTrue)
                    .help("Emit machine-readable JSON"),
            ),
    );

    cmd = cmd.subcommand(
        Command::new("tree")
            .about("Show full command tree")
            .arg(
                Arg::new("json")
                    .long("json")
                    .action(ArgAction::SetTrue)
                    .help("Emit machine-readable JSON"),
            ),
    );

    cmd = cmd.subcommand(
        Command::new("gaql")
            .about("GAQL search")
            .subcommand_required(true)
            .subcommand(
                Command::new("search")
                    .about("Search with GAQL")
                    .arg(
                        Arg::new("customer_id")
                            .long("customer-id")
                            .value_name("ID")
                            .help("Customer id (env: GOOGLE_ADS_CUSTOMER_ID)"),
                    )
                    .arg(
                        Arg::new("query")
                            .long("query")
                            .value_name("GAQL")
                            .required(true)
                            .help("GAQL query"),
                    )
                    .arg(
                        Arg::new("use_search")
                            .long("use-search")
                            .action(ArgAction::SetTrue)
                            .help("Use Search (unary) instead of SearchStream"),
                    )
                    .arg(
                        Arg::new("page_size")
                            .long("page-size")
                            .value_parser(value_parser!(i64))
                            .help("Page size for Search"),
                    )
                    .arg(
                        Arg::new("page_token")
                            .long("page-token")
                            .value_name("TOKEN")
                            .help("Page token for Search"),
                    )
                    .arg(
                        Arg::new("validate_only")
                            .long("validate-only")
                            .action(ArgAction::SetTrue)
                            .help("Validate only"),
                    )
                    .arg(
                        Arg::new("summary_row_setting")
                            .long("summary-row-setting")
                            .value_name("SETTING")
                            .help("Summary row setting enum"),
                    )
                    .arg(
                        Arg::new("return_total_results_count")
                            .long("return-total-results-count")
                            .action(ArgAction::SetTrue)
                            .help("Return total results count"),
                    ),
            ),
    );

    cmd = cmd.subcommand(
        Command::new("mutate")
            .about("Mutate resources via GoogleAdsService.Mutate")
            .arg(
                Arg::new("customer_id")
                    .long("customer-id")
                    .value_name("ID")
                    .help("Customer id (env: GOOGLE_ADS_CUSTOMER_ID)"),
            )
            .arg(
                Arg::new("ops")
                    .long("ops")
                    .value_name("JSON")
                    .help("MutateOperations array (JSON or @file)")
                    .conflicts_with("body"),
            )
            .arg(
                Arg::new("body")
                    .long("body")
                    .value_name("JSON")
                    .help("Full request body JSON (or @file)")
                    .conflicts_with("ops"),
            )
            .arg(
                Arg::new("partial_failure")
                    .long("partial-failure")
                    .action(ArgAction::SetTrue)
                    .help("Enable partial failure"),
            )
            .arg(
                Arg::new("validate_only")
                    .long("validate-only")
                    .action(ArgAction::SetTrue)
                    .help("Validate only"),
            )
            .arg(
                Arg::new("response_content_type")
                    .long("response-content-type")
                    .value_name("TYPE")
                    .help("Response content type enum"),
            ),
    );

    cmd = cmd.subcommand(
        Command::new("raw")
            .about("Raw gRPC call using JSON body")
            .arg(
                Arg::new("service")
                    .long("service")
                    .required(true)
                    .value_name("SERVICE")
                    .help("Service name (e.g. google-ads-service)"),
            )
            .arg(
                Arg::new("method")
                    .long("method")
                    .required(true)
                    .value_name("METHOD")
                    .help("Method name (e.g. search-stream)"),
            )
            .arg(
                Arg::new("body")
                    .long("body")
                    .required(true)
                    .value_name("JSON")
                    .help("Request body JSON (or @file)"),
            ),
    );

    if tree.services.is_empty() {
        return cmd;
    }

    cmd
}

fn handle_list(tree: &CommandTree, matches: &clap::ArgMatches) -> Result<()> {
    if matches.get_flag("json") {
        return write_json(&json!(tree.services), true);
    }

    for service in &tree.services {
        write_stdout_line(&service.name)?;
        for method in &service.methods {
            write_stdout_line(&format!("  {}", method.name))?;
        }
    }
    Ok(())
}

fn handle_describe(pool: &prost_reflect::DescriptorPool, matches: &clap::ArgMatches) -> Result<()> {
    let service = matches
        .get_one::<String>("service")
        .ok_or_else(|| anyhow!("service required"))?;
    let method = matches
        .get_one::<String>("method")
        .ok_or_else(|| anyhow!("method required"))?;

    let description = describe_method(pool, service, method)?;
    if matches.get_flag("json") {
        return write_json(&serde_json::to_value(description)?, true);
    }

    write_stdout_line(&format!("{} {}", description.service, description.method))?;
    write_stdout_line("fields:")?;
    for field in description.fields {
        write_stdout_line(&format!("  {} ({})", field.json_name, field.kind))?;
    }
    Ok(())
}

fn handle_tree(tree: &CommandTree, matches: &clap::ArgMatches) -> Result<()> {
    if matches.get_flag("json") {
        return write_json(&serde_json::to_value(tree)?, true);
    }

    write_stdout_line(&format!("api_version: {}", tree.api_version))?;
    for service in &tree.services {
        write_stdout_line(&service.name)?;
        for method in &service.methods {
            write_stdout_line(&format!("  {}", method.name))?;
        }
    }
    Ok(())
}

fn write_gaql_output(output: GaqlOutput, pretty: bool) -> Result<()> {
    match output {
        GaqlOutput::Json(value) => write_json(&value, pretty),
        GaqlOutput::JsonLines(values) => {
            for value in values {
                write_stdout_line(&serde_json::to_string(&value)?)?;
            }
            Ok(())
        }
    }
}

fn write_json(value: &Value, pretty: bool) -> Result<()> {
    if pretty {
        write_stdout_line(&serde_json::to_string_pretty(value)?)?;
    } else {
        write_stdout_line(&serde_json::to_string(value)?)?;
    }
    Ok(())
}

fn write_stdout_line(line: &str) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(line.as_bytes())?;
    stdout.write_all(b"\n")?;
    Ok(())
}

fn read_customer_id(matches: &clap::ArgMatches) -> Result<String> {
    let value = matches
        .get_one::<String>("customer_id")
        .cloned()
        .or_else(|| env::var("GOOGLE_ADS_CUSTOMER_ID").ok())
        .ok_or_else(|| anyhow!("--customer-id or GOOGLE_ADS_CUSTOMER_ID required"))?;
    Ok(normalize_customer_id(&value))
}

struct Config {
    developer_token: String,
    endpoint: String,
    login_customer_id: Option<String>,
    access_token: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    refresh_token: Option<String>,
}

fn load_config(matches: &clap::ArgMatches) -> Result<Config> {
    if matches.get_flag("debug") {
        env_logger::Builder::from_env("RUST_LOG")
            .filter_level(log::LevelFilter::Debug)
            .init();
    } else {
        env_logger::Builder::from_env("RUST_LOG")
            .filter_level(log::LevelFilter::Warn)
            .init();
    }

    let developer_token = matches
        .get_one::<String>("developer_token")
        .cloned()
        .or_else(|| env::var("GOOGLE_ADS_DEVELOPER_TOKEN").ok())
        .ok_or_else(|| anyhow!("GOOGLE_ADS_DEVELOPER_TOKEN missing"))?;

    let access_token = matches
        .get_one::<String>("access_token")
        .cloned()
        .or_else(|| env::var("GOOGLE_ADS_ACCESS_TOKEN").ok());

    let client_id = matches
        .get_one::<String>("client_id")
        .cloned()
        .or_else(|| env::var("GOOGLE_ADS_CLIENT_ID").ok());

    let client_secret = matches
        .get_one::<String>("client_secret")
        .cloned()
        .or_else(|| env::var("GOOGLE_ADS_CLIENT_SECRET").ok());

    let refresh_token = matches
        .get_one::<String>("refresh_token")
        .cloned()
        .or_else(|| env::var("GOOGLE_ADS_REFRESH_TOKEN").ok());

    let login_customer_id = matches
        .get_one::<String>("login_customer_id")
        .cloned()
        .or_else(|| env::var("GOOGLE_ADS_LOGIN_CUSTOMER_ID").ok())
        .map(|value| normalize_customer_id(&value));

    let endpoint = matches
        .get_one::<String>("endpoint")
        .cloned()
        .or_else(|| env::var("GOOGLE_ADS_ENDPOINT").ok())
        .unwrap_or_else(|| "https://googleads.googleapis.com".to_string());

    Ok(Config {
        developer_token,
        endpoint,
        login_customer_id,
        access_token,
        client_id,
        client_secret,
        refresh_token,
    })
}
