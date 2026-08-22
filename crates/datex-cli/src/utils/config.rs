use colored::Colorize;
use datex_core::{
    datex_proxy::{
        DatexValueContainerProxyDeserialize,
        DatexValueContainerProxyInfallibleSerialize, DeserializationError,
    },
    decompiler::{DecompileOptions, FormattingOptions, decompile_value},
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
    let config = config.to_value_container_without_cache();
    let datex_script = decompile_value(
        &config,
        DecompileOptions {
            formatting_options: FormattingOptions::default(),
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

pub fn get_config(
    custom_config_path: Option<&PathBuf>,
    runtime: &Runtime,
) -> Result<RuntimeConfig, ConfigError> {
    Ok(match custom_config_path {
        Some(path) => read_config_file(path, runtime)?,
        None => {
            match home::home_dir() {
                Some(path) if !path.as_os_str().is_empty() => {
                    // get all .dx files in the home directory .datex folder
                    let dx_files = get_dx_files(path.clone())?;
                    // if no files yet, create a new config file for a random endpoint
                    if dx_files.is_empty() {
                        let endpoint = Endpoint::random();
                        let config_path =
                            create_new_config_file(path.clone(), endpoint)?;
                        read_config_file(&config_path, runtime)?
                    } else {
                        // if there are files, read the first one
                        let config_path = dx_files.first().unwrap().clone();
                        println!(
                            "Using endpoint config file {}",
                            config_path.to_str().unwrap()
                        );
                        read_config_file(&config_path, runtime)?
                    }
                }
                _ => {
                    eprintln!(
                        "Unable to get home directory, using temporary endpoint."
                    );
                    RuntimeConfig::new_with_endpoint(Endpoint::random())
                }
            }
        }
    })
}

pub async fn run_runtime_with_config<AppReturn, AppFuture>(
    custom_config_path: Option<&PathBuf>,
    print_header: bool,
    app_logic: impl FnOnce(Runtime) -> AppFuture,
) -> Result<AppReturn, ConfigError>
where
    AppFuture: Future<Output = AppReturn>,
{
    let mut config = get_config(custom_config_path, &Runtime::stub())?;
    config.load_host_env_vars();

    let runner = RuntimeRunner::new(config);
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
