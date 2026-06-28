use crate::utils::{
    config::{ConfigError, run_runtime_with_config},
    paths::get_datex_base_dir,
};
use datex_core::{
    decompiler::{
        DecompileOptions, FormattingMode, FormattingOptions,
        apply_syntax_highlighting, decompile_value,
    },
    runtime::{
        Runtime,
        execution::{
            context::{ExecutionContext, ExecutionMode, ScriptExecutionError},
            execution_input::ExecutionCallerMetadata,
        },
    },
    values::core_values::endpoint::Endpoint,
};
use rustyline::{
    Helper,
    completion::Completer,
    config::Configurer,
    error::ReadlineError,
    highlight::{CmdKind, Highlighter},
    hint::Hinter,
    validate::{ValidationContext, ValidationResult, Validator},
};
use std::{path::PathBuf, str::FromStr, thread::spawn};

struct DatexSyntaxHelper;

impl Highlighter for DatexSyntaxHelper {
    fn highlight<'l>(
        &self,
        line: &'l str,
        _pos: usize,
    ) -> std::borrow::Cow<'l, str> {
        std::borrow::Cow::Owned(
            apply_syntax_highlighting(line.to_string()).unwrap(),
        )
    }
    fn highlight_char(&self, _line: &str, _pos: usize, _kind: CmdKind) -> bool {
        true
    }
}

impl Validator for DatexSyntaxHelper {
    fn validate(
        &self,
        _ctx: &mut ValidationContext,
    ) -> rustyline::Result<ValidationResult> {
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
    // if verbose mode is enabled, set log level to debug, otherwise set it to warn
    let log_level = if options.verbose { "info" } else { "warn" };
    flexi_logger::Logger::try_with_env_or_str(log_level)
        .unwrap()
        .start()
        .unwrap();

    let (cmd_sender, mut cmd_receiver) =
        tokio::sync::mpsc::channel::<ReplCommand>(100);
    let (response_sender, response_receiver) =
        tokio::sync::mpsc::channel::<ReplResponse>(100);

    let res: Result<Result<(), ReplError>, ConfigError> = run_runtime_with_config(options.config_path.as_ref(), true, async |runtime: Runtime| {

        repl_loop(cmd_sender, response_receiver, get_datex_base_dir().unwrap())?;

        // create context
        let mut execution_context = if options.verbose {
            ExecutionContext::local_debug(ExecutionMode::unbounded(), runtime.clone(), ExecutionCallerMetadata::local_default())
        } else {
            ExecutionContext::local(ExecutionMode::unbounded(), runtime.clone(), ExecutionCallerMetadata::local_default())
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
                        let decompiled_value = decompile_value(&result, DecompileOptions {
                            formatting_options: FormattingOptions {
                                mode: FormattingMode::pretty(),
                                json_compat: false,
                                colorized: true,
                                add_variant_suffix: true
                            },
                            resolve_slots: true,
                        });
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
    }).await;
    res.map_err(ReplError::ConfigError)?
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
    datex_base_path: PathBuf,
) -> Result<(), ReplError> {
    let mut history_cache_path = datex_base_path.clone();
    history_cache_path.push("repl-history.txt");

    let mut rl = rustyline::Editor::<DatexSyntaxHelper, _>::new()?;
    if rl.load_history(&history_cache_path).is_ok() {}
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
                            sender
                                .blocking_send(ReplCommand::ComHubInfo)
                                .unwrap();
                        }
                        "mem" => {
                            sender
                                .blocking_send(ReplCommand::LocalMemoryDump)
                                .unwrap();
                        }
                        _ => {
                            // if starting with "trace", send trace command
                            if line.starts_with("trace ") {
                                let endpoint = Endpoint::from_str(&line[6..]);
                                if endpoint.is_err() {
                                    println!(
                                        "Invalid endpoint format. Use 'trace <endpoint>'."
                                    );
                                    continue;
                                }
                                sender
                                    .blocking_send(ReplCommand::Trace(
                                        endpoint.unwrap(),
                                    ))
                                    .unwrap();
                            } else {
                                sender
                                    .blocking_send(ReplCommand::Execute(
                                        line.clone(),
                                    ))
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

        // Save history on exit
        rl.save_history(&history_cache_path).unwrap();
    });

    Ok(())
}
