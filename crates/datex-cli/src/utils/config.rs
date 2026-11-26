use colored::Colorize;
use datex_core::{
    decompiler::{DecompileOptions, FormattingOptions},
    network::{
        com_hub::InterfacePriority,
        com_interfaces::default_setup_data::websocket::websocket_client::WebSocketClientInterfaceSetupData,
    },
    runtime::{Runtime, RuntimeConfig, RuntimeRunner},
    values::core_values::endpoint::Endpoint,
};
use datex_native::com_interfaces::register_native_interface_factories;
use std::{
    fs,
    path::{Path, PathBuf},
};
use datex_core::decompiler::ast_to_source_code::value_to_source_code;
use datex_core::runtime::CustomRuntimeInfo;
use datex_core::traits::convert_value_container::{ConvertValueContainer, DeserializationError};

#[derive(Debug)]
pub enum ConfigError {
    DeserializationError(DeserializationError),
    IOError(std::io::Error),
}

impl From<std::io::Error> for ConfigError {
    fn from(err: std::io::Error) -> Self {
        ConfigError::IOError(err)
    }
}

impl From<DeserializationError> for ConfigError {
    fn from(err: DeserializationError) -> Self {
        ConfigError::DeserializationError(err)
    }
}

pub fn read_config_file(
    path: &Path,
    runtime: &Runtime,
) -> Result<RuntimeConfig, DeserializationError> {
    let config: RuntimeConfig = RuntimeConfig::try_from_dx_file(path, runtime)?;
    Ok(config)
}

fn get_dx_files(base_path: PathBuf) -> Result<Vec<PathBuf>, ConfigError> {
    let mut config_dir = base_path.clone();
    config_dir.push(".datex");

    // Create the directory if it doesn't exist
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)?;
    }

    // Collect all files ending with `.dx`
    let dx_files = fs::read_dir(&config_dir)?
        .filter_map(|entry| {
            entry.ok().and_then(|e| {
                let path = e.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("dx") {
                    Some(path)
                } else {
                    None
                }
            })
        })
        .collect();

    Ok(dx_files)
}

pub fn create_new_config_file(
    base_path: PathBuf,
    endpoint: Endpoint,
) -> Result<PathBuf, ConfigError> {
    let mut config = RuntimeConfig::new_with_endpoint(endpoint.clone());

    // add default interface
    config.add_interface(
        "websocket-client".to_string(),
        WebSocketClientInterfaceSetupData {
            url: "wss://example.unyt.land".to_string(),
        },
        InterfacePriority::default(),
    );

    let mut config_path = base_path.clone();
    config_path.push(".datex");
    config_path.push(format!("{endpoint}.dx"));
    let config = config.to_value_container();
    let datex_script = value_to_source_code(
        &config,
        DecompileOptions {
            formatting_options: FormattingOptions::pretty(),
            ..DecompileOptions::default()
        },
    );
    fs::write(config_path.clone(), datex_script)?;

    println!(
        "Created new config file at {}",
        config_path.to_str().unwrap()
    );

    Ok(config_path)
}

/// Load the config from the given path, or return a default config if no path is given.
/// Also initialize the env variables and sets the vault location if none is set in the config.
pub fn get_config(
    custom_config_path: Option<&PathBuf>,
    runtime: &Runtime,
) -> Result<RuntimeConfig, ConfigError> {
    let mut config = match custom_config_path {
        Some(path) => read_config_file(path, runtime)?,
        None => RuntimeConfig::default(),
    };

    config.load_host_env_vars();

    // set path to the .datex folder if no vault_location is set in the config
    if config.vault_location.is_none() && let Some(home_dir) = home::home_dir() {
        let mut vault_path = home_dir.clone();
        vault_path.push(".datex");
        config.vault_location = Some(vault_path.to_string_lossy().into_owned());
    }
    
    Ok(config)
}

pub async fn run_runtime_with_config<AppReturn, AppFuture>(
    custom_config_path: Option<&PathBuf>,
    print_header: bool,
    app_logic: impl FnOnce(Runtime) -> AppFuture,
) -> Result<AppReturn, ConfigError>
where
    AppFuture: Future<Output = AppReturn>,
{
    let config = get_config(custom_config_path, &Runtime::stub())?;

    let device_info = os_info::get();

    let runner = RuntimeRunner::new(config, CustomRuntimeInfo {
        device_type: Some(device_info.to_string()),
        ..Default::default()
    }).await;
    register_native_interface_factories(&runner.runtime.com_hub());

    Ok(runner
        .run(async |runtime: Runtime| {
            if print_header {
                print_runtime_header(&runtime);
            }

            app_logic(runtime).await
        })
        .await)
}

fn print_runtime_header(runtime: &Runtime) {
    let endpoint_str_no_color = format!(" Endpoint: {} ", runtime.endpoint());
    let endpoint_str = format!(
        " Endpoint: {} ",
        runtime.endpoint().to_string().truecolor(88, 212, 82)
    );
    let width = endpoint_str_no_color.len().max(20);

    println!("┌{}┐", "─".repeat(width));
    println!(
        "│{:<width$}│",
        format!(" DATEX v{}", runtime.version()),
        width = width
    );
    println!(
        "│{:<width$}│",
        endpoint_str,
        width = width + endpoint_str.len() - endpoint_str_no_color.len()
    );
    println!("└{}┘", "─".repeat(width));
}
