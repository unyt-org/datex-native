use datex_core::compiler::workspace::CompilerWorkspace;
use datex_core::crypto::crypto_native::CryptoNative;
use datex_core::decompiler::{DecompileOptions, decompile_value};
use datex_core::run_async;
use datex_core::runtime::global_context::{DebugFlags, GlobalContext, set_global_context};
use datex_core::runtime::{Runtime, RuntimeConfig};
use datex_core::utils::time_native::TimeNative;
use datex_core::values::core_values::endpoint::Endpoint;
use std::path::PathBuf;
use std::sync::Arc;

mod command_line_args;
mod lsp;
mod repl;
mod utils;
mod workbench;

use crate::command_line_args::Repl;
use crate::lsp::LanguageServerBackend;
use crate::repl::{ReplOptions, repl};
use crate::utils::config::{ConfigError, create_runtime_with_config};
use command_line_args::{Subcommands, get_command};
use realhydroper_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let command = get_command();

    // print version
    let command = if command.version {
        println!("datex-cli {}", env!("CARGO_PKG_VERSION"));
        println!("datex {}", env!("DEP_DATEX_CORE_VERSION"));
        return;
    } else {
        command.command
    };

    if let Some(cmd) = command {
        match cmd {
            Subcommands::Lsp(lsp) => {
                let stdin = tokio::io::stdin();
                let stdout = tokio::io::stdout();

                let runtime = Runtime::new(RuntimeConfig::new_with_endpoint(Endpoint::default()));
                let compiler_workspace = CompilerWorkspace::new(runtime);

                let (service, socket) = LspService::new(|client| {
                    LanguageServerBackend::new(client, compiler_workspace)
                });
                Server::new(stdin, stdout, socket).serve(service).await;
            }
            Subcommands::Run(run) => {
                execute_file(run).await;
            }
            Subcommands::Repl(Repl { verbose, config }) => {
                let options = ReplOptions {
                    verbose,
                    config_path: config,
                };
                repl(options).await.unwrap();
            }
            Subcommands::Workbench(_) => {
                workbench(None, false).await.expect("Workbench failed");
            }
        }
    }
    // run REPL if no command is provided
    else {
        repl(ReplOptions::default()).await.unwrap();
    }
}

async fn execute_file(run: command_line_args::Run) {
    run_async! {
        if let Some(file) = run.file {
            let runtime = create_runtime_with_config(run.config, false, false).await.unwrap();
            // yield to wait for connect. TODO: better way
            tokio::task::yield_now().await;
            let file_contents = std::fs::read_to_string(file).expect("Could not read file");
            let _result = runtime.execute(&file_contents, &[], None).await;
            if let Err(e) = _result {
                eprintln!("{}", e);
            }
            else {
                let result = _result.unwrap();
                if let Some(output) = result {
                    let formatted_output = decompile_value(
                        &output,
                        DecompileOptions::colorized()
                    );
                    println!("{}", formatted_output);
                }
            }
        }
        else {
            eprintln!("No file provided to run.");
        }
    }
}

async fn workbench(config_path: Option<PathBuf>, debug: bool) -> Result<(), ConfigError> {
    set_global_context(GlobalContext {
        crypto: Arc::new(CryptoNative),
        time: Arc::new(TimeNative),
        debug_flags: DebugFlags::default(),
    });

    run_async! {
        let runtime = create_runtime_with_config(config_path, debug, false).await?;
        workbench::start_workbench(runtime).await?;

        Ok(())
    }
}
