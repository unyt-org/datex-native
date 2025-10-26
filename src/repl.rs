use crate::utils::config::{create_runtime_with_config, ConfigError};
use datex_core::crypto::crypto_native::CryptoNative;
use datex_core::decompiler::{DecompileOptions, apply_syntax_highlighting, decompile_value};
use datex_core::run_async;
use datex_core::runtime::execution_context::{ExecutionContext, ScriptExecutionError};
use datex_core::runtime::global_context::{GlobalContext, set_global_context};
use datex_core::utils::time_native::TimeNative;
use datex_core::values::core_values::endpoint::Endpoint;
use rustyline::Helper;
use rustyline::completion::Completer;
use rustyline::config::Configurer;
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::Hinter;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::thread::spawn;

struct DatexSyntaxHelper;

impl Highlighter for DatexSyntaxHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> std::borrow::Cow<'l, str> {
        std::borrow::Cow::Owned(apply_syntax_highlighting(line.to_string()).unwrap())
    }
    fn highlight_char(&self, line: &str, pos: usize, kind: CmdKind) -> bool {
        true
    }
}

// ref x = {}
// val x = (1,2,3,r);
// val y: ((string|decimal): number)  = ("sadf":234)
// const val x = 10;
// ref x = {};
// x.a = 10;
// ref y = (1,2,3); // Map
// y.x = 10;
// func (1,2,3)

// ref weather: Weather;
// weather = getWeatherFromApi(); -> val
// weather = always cpnvertWearth(getWeatherFromApi()); -> indirect copy

// ref user: User; <-- $user
// #0 <- $user
// for name in endpoint (
//    user = resolveInner/innerRef/collapse/resolve getUserFromApi(name); $a -> $b -> $c;
// )
// user // <- $x
// val x = 10;

// ref x = weather;

// (1: x) == ($(1): x, 1: x)
// (val string: any)
// {x: 1} == {0: x, (0min): 20m}
// x.y  -> (y: 34)
// x.({a}) -> ({a}: 4)

impl Validator for DatexSyntaxHelper {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        Ok(ValidationResult::Valid(None))
    }
    fn validate_while_typing(&self) -> bool {
        true
    }
}
impl Completer for DatexSyntaxHelper {
    type Candidate = String;
}
impl Hinter for DatexSyntaxHelper {
    type Hint = String;
}
impl Helper for DatexSyntaxHelper {}

#[derive(Debug, Clone, Default)]
pub struct ReplOptions {
    pub verbose: bool,
    pub config_path: Option<PathBuf>,
}

#[derive(Debug)]
pub enum ReplError {
    ReadlineError(ReadlineError),
    ConfigError(ConfigError),
}

impl From<ReadlineError> for ReplError {
    fn from(err: ReadlineError) -> Self {
        ReplError::ReadlineError(err)
    }
}
impl From<ConfigError> for ReplError {
    fn from(err: ConfigError) -> Self {
        ReplError::ConfigError(err)
    }
}

pub async fn repl(options: ReplOptions) -> Result<(), ReplError> {
    set_global_context(GlobalContext::new(
        Arc::new(CryptoNative),
        Arc::new(TimeNative),
    ));

    let (cmd_sender, mut cmd_receiver) = tokio::sync::mpsc::channel::<ReplCommand>(100);
    let (response_sender, response_receiver) = tokio::sync::mpsc::channel::<ReplResponse>(100);

    run_async! {
        let runtime = create_runtime_with_config(options.config_path, options.verbose, true).await?;

        repl_loop(cmd_sender, response_receiver)?;

        // create context
        let mut execution_context = if options.verbose {
            ExecutionContext::local_debug(false)
        } else {
            ExecutionContext::local()
        };

        while let Some(command) = cmd_receiver.recv().await {
            match command {
                ReplCommand::ComHubInfo => {
                    let metadata = runtime.com_hub().get_metadata().to_string();
                    response_sender.send(ReplResponse::Result(Some(metadata))).await.unwrap();
                }
                ReplCommand::LocalMemoryDump => {
                    let metadata = execution_context.memory_dump();
                    if let Some(metadata) = metadata {
                        let metadata = format!("Memory Dump:\n\n{metadata}");
                        response_sender.send(ReplResponse::Result(Some(metadata))).await.unwrap();
                    }
                    else {
                        response_sender.send(ReplResponse::Result(Some("<Memory dump not available>".to_string()))).await.unwrap();
                    }
                }
                ReplCommand::Trace(endpoint) => {
                    let trace = runtime.com_hub().record_trace(endpoint).await;
                    match trace {
                        Some(trace) => {
                            let trace_string = trace.to_string();
                            response_sender.send(ReplResponse::Result(Some(trace_string)))
                                .await.unwrap();
                        }
                        None => {
                            response_sender.send(ReplResponse::Result(Some("Could not create trace".to_string()))).await.unwrap();
                        }
                    }
                }
                ReplCommand::Execute(line) => {
                    let result = runtime.execute(&line, &[], Some(&mut execution_context)).await;

                    let mut result_string = None;

                    if let Err(e) = result {
                        match e {
                            ScriptExecutionError::CompilerError(e) => {
                                result_string = Some(format!("\x1b[31m[Compiler Error] {e}\x1b[0m"));
                            }
                            ScriptExecutionError::ExecutionError(e) => {
                                result_string = Some(format!("\x1b[31m[Execution Error] {e}\x1b[0m"));
                            }
                        }
                    }

                    else if let Some(result) = result.unwrap() {
                        let decompiled_value = decompile_value(&result, DecompileOptions::colorized());
                        // indent all lines except the first with 2 spaces to match the REPL prompt indentation
                        let decompiled_value = decompiled_value.lines().enumerate().map(|(i, line)| {
                            if i == 0 {
                                line.to_string()
                            } else {
                                format!("  {line}")
                            }
                        }).collect::<Vec<String>>().join("\n");
                        result_string = Some(format!("< {decompiled_value}"));
                    }
                    else {
                        result_string = None;
                    }

                    response_sender.send(ReplResponse::Result(result_string)).await.unwrap();
                }
            }
        }

        Ok(())
    }
}

enum ReplCommand {
    ComHubInfo,
    LocalMemoryDump,
    Trace(Endpoint),
    Execute(String),
}

enum ReplResponse {
    Result(Option<String>),
}

fn repl_loop(
    sender: tokio::sync::mpsc::Sender<ReplCommand>,
    mut receiver: tokio::sync::mpsc::Receiver<ReplResponse>,
) -> Result<(), ReplError> {
    let mut rl = rustyline::Editor::<DatexSyntaxHelper, _>::new()?;
    rl.set_helper(Some(DatexSyntaxHelper));
    rl.enable_bracketed_paste(true);
    rl.set_auto_add_history(true);

    spawn(move || {
        loop {
            let readline = rl.readline("> ");
            match readline {
                Ok(line) => {
                    match line.trim() {
                        "clear" => {
                            rl.clear_screen().unwrap();
                            continue;
                        }
                        "com" => {
                            sender.blocking_send(ReplCommand::ComHubInfo).unwrap();
                        }
                        "mem" => {
                            sender.blocking_send(ReplCommand::LocalMemoryDump).unwrap();
                        }
                        _ => {
                            // if starting with "trace", send trace command
                            if line.starts_with("trace ") {
                                let endpoint = Endpoint::from_str(&line[6..]);
                                if endpoint.is_err() {
                                    println!("Invalid endpoint format. Use 'trace <endpoint>'.");
                                    continue;
                                }
                                sender
                                    .blocking_send(ReplCommand::Trace(endpoint.unwrap()))
                                    .unwrap();
                            } else {
                                sender
                                    .blocking_send(ReplCommand::Execute(line.clone()))
                                    .unwrap();
                            }
                        }
                    }
                }
                Err(_) => break,
            }

            let response = receiver.blocking_recv();
            match response {
                Some(ReplResponse::Result(result)) => {
                    if let Some(result) = result {
                        println!("{result}");
                    }
                }
                None => {
                    break;
                }
            }
        }
    });

    Ok(())
}
