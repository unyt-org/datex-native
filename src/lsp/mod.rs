mod type_hint_collector;
mod utils;
mod variable_declaration_finder;

use crate::lsp::variable_declaration_finder::VariableDeclarationFinder;
use datex_core::ast::structs::expression::{
    DatexExpressionData, VariableAccess, VariableAssignment, VariableDeclaration,
};
use datex_core::compiler::error::CompilerError;
use datex_core::compiler::workspace::CompilerWorkspace;
use datex_core::compiler::precompiler::precompiled_ast::RichAst;
use datex_core::types::type_container::TypeContainer;
use datex_core::visitor::expression::ExpressionVisitor;
use realhydroper_lsp::jsonrpc::{Error, ErrorCode};
use realhydroper_lsp::lsp_types::*;
use realhydroper_lsp::{Client, LanguageServer};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct SpannedCompilerError {
    pub range: Range,
    pub error: CompilerError,
}

pub struct LanguageServerBackend {
    pub client: Client,
    pub compiler_workspace: RefCell<CompilerWorkspace>,
    pub spanned_compiler_errors: RefCell<HashMap<PathBuf, Vec<SpannedCompilerError>>>,
}

impl LanguageServerBackend {
    pub fn new(client: Client, compiler_workspace: CompilerWorkspace) -> Self {
        Self {
            client,
            compiler_workspace: RefCell::new(compiler_workspace),
            spanned_compiler_errors: RefCell::new(HashMap::new()),
        }
    }
}

#[realhydroper_lsp::async_trait(?Send)]
impl LanguageServer for LanguageServerBackend {
    async fn initialize(
        &self,
        _: InitializeParams,
    ) -> realhydroper_lsp::jsonrpc::Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions::default()),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        inter_file_dependencies: true,
                        workspace_diagnostics: false,
                        identifier: None,
                        work_done_progress_options: WorkDoneProgressOptions {
                            work_done_progress: None,
                        },
                    },
                )),
                inlay_hint_provider: Some(OneOf::Left(true)),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(true),
                    work_done_progress_options: Default::default(),
                }),
                definition_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn shutdown(&self) -> realhydroper_lsp::jsonrpc::Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("File opened: {}", params.text_document.uri),
            )
            .await;

        self.update_file_contents(
            params.text_document.uri.to_file_path().unwrap(),
            params.text_document.text,
        )
        .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("File changed: {}", params.text_document.uri),
            )
            .await;
        let new_content = params
            .content_changes
            .into_iter()
            .next()
            .map(|change| change.text)
            .unwrap_or_default();
        self.update_file_contents(
            params.text_document.uri.to_file_path().unwrap(),
            new_content,
        )
        .await;
    }

    async fn completion(
        &self,
        params: CompletionParams,
    ) -> realhydroper_lsp::jsonrpc::Result<Option<CompletionResponse>> {
        self.client
            .log_message(MessageType::INFO, "completion!")
            .await;

        let position = params.text_document_position;

        // For simplicity, we assume the prefix is the last word before the cursor.
        // In a real implementation, you would extract this from the document content.
        let prefix = self.get_previous_text_at_position(&position);
        self.client
            .log_message(MessageType::INFO, format!("Completion prefix: {}", prefix))
            .await;

        let variables = self.find_variable_starting_with(&prefix);

        let items: Vec<CompletionItem> = variables
            .iter()
            .map(|var| CompletionItem {
                label: var.name.clone(),
                kind: Some(CompletionItemKind::VARIABLE),
                detail: Some(format!(
                    "{} {}: {}",
                    var.shape,
                    var.name,
                    var.var_type.as_ref().unwrap()
                )),
                documentation: None,
                ..Default::default()
            })
            .collect();

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> realhydroper_lsp::jsonrpc::Result<Option<Hover>> {
        let expression = self.get_expression_at_position(&params.text_document_position_params);

        if let Some(expression) = expression {
            Ok(match expression.data {
                // show variable type info on hover
                DatexExpressionData::VariableDeclaration(VariableDeclaration {
                    name,
                    id: Some(id),
                    ..
                })
                | DatexExpressionData::VariableAssignment(VariableAssignment {
                    name,
                    id: Some(id),
                    ..
                })
                | DatexExpressionData::VariableAccess(VariableAccess { id, name }) => {
                    let variable_metadata = self.get_variable_by_id(id).unwrap();
                    Some(self.get_language_string_hover(&format!(
                        "{} {}: {}",
                        variable_metadata.shape,
                        name,
                        variable_metadata.var_type.unwrap_or(TypeContainer::unknown())
                    )))
                }

                // show value info on hover for literals
                DatexExpressionData::Integer(integer) => {
                    Some(self.get_language_string_hover(&format!("{}", integer)))
                }
                DatexExpressionData::TypedInteger(typed_integer) => {
                    Some(self.get_language_string_hover(&format!("{}", typed_integer)))
                }
                DatexExpressionData::Decimal(decimal) => {
                    Some(self.get_language_string_hover(&format!("{}", decimal)))
                }
                DatexExpressionData::TypedDecimal(typed_decimal) => {
                    Some(self.get_language_string_hover(&format!("{}", typed_decimal)))
                }
                DatexExpressionData::Boolean(boolean) => {
                    Some(self.get_language_string_hover(&format!("{}", boolean)))
                }
                DatexExpressionData::Text(text) => {
                    Some(self.get_language_string_hover(&format!("\"{}\"", text)))
                }
                DatexExpressionData::Endpoint(endpoint) => {
                    Some(self.get_language_string_hover(&format!("{}", endpoint)))
                }
                DatexExpressionData::Null => Some(self.get_language_string_hover("null")),

                _ => None,
            })
        } else {
            Err(realhydroper_lsp::jsonrpc::Error {
                code: ErrorCode::ParseError,
                message: Cow::from("No AST available"),
                data: None,
            })
        }
    }

    async fn inlay_hint(
        &self,
        params: InlayHintParams,
    ) -> realhydroper_lsp::jsonrpc::Result<Option<Vec<InlayHint>>> {
        // show type hints for variables
        let type_hints = self
            .get_type_hints(params.text_document.uri.to_file_path().unwrap())
            .unwrap()
            .into_iter()
            .map(|hint| InlayHint {
                position: hint.0,
                label: InlayHintLabel::String(format!(": {}", hint.1.unwrap())),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: None,
                padding_left: Some(true),
                padding_right: None,
                data: None,
            })
            .collect();

        Ok(Some(type_hints))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> realhydroper_lsp::jsonrpc::Result<Option<GotoDefinitionResponse>> {
        let expression = self.get_expression_at_position(&params.text_document_position_params);
        if let Some(expression) = expression {
            match expression.data {
                DatexExpressionData::VariableAccess(VariableAccess { id, name }) => {
                    let uri = params.text_document_position_params.text_document.uri;
                    let file_path = uri.to_file_path().unwrap();
                    let mut workspace = self.compiler_workspace.borrow_mut();
                    let file = workspace.get_file_mut(&file_path).unwrap();
                    if let Some(RichAst { ast, .. }) = &mut file.rich_ast {
                        let mut finder = VariableDeclarationFinder::new(id);
                        finder.visit_datex_expression(ast);
                        Ok(finder.variable_declaration_position.map(|position| {
                            GotoDefinitionResponse::Scalar(Location {
                                uri,
                                range: self
                                    .convert_byte_range_to_document_range(&position, &file.content),
                            })
                        }))
                    } else {
                        Ok(None)
                    }
                }
                _ => Ok(None),
            }
        } else {
            Err(Error::internal_error())
        }
    }

    async fn document_link(
        &self,
        params: DocumentLinkParams,
    ) -> realhydroper_lsp::jsonrpc::Result<Option<Vec<DocumentLink>>> {
        // TODO
        Ok(Some(vec![]))
    }

    // get error diagnostics
    async fn diagnostic(
        &self,
        params: DocumentDiagnosticParams,
    ) -> realhydroper_lsp::jsonrpc::Result<DocumentDiagnosticReportResult> {
        self.client
            .log_message(MessageType::INFO, "diagnostics!")
            .await;

        let uri = params.text_document.uri;
        let file_path = uri.to_file_path().unwrap();

        let diagnostics = self.get_diagnostics_for_file(&file_path);
        let report = FullDocumentDiagnosticReport {
            result_id: None,
            items: diagnostics,
        };

        Ok(DocumentDiagnosticReportResult::Report(
            DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: report,
            }),
        ))
    }
}

impl LanguageServerBackend {
    fn get_language_string_hover(&self, text: &str) -> Hover {
        let contents = HoverContents::Scalar(MarkedString::LanguageString(LanguageString {
            language: "datex".to_string(),
            value: text.to_string(),
        }));
        Hover {
            contents,
            range: None,
        }
    }

    fn get_diagnostics_for_file(&self, file_path: &std::path::Path) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let errors = self.spanned_compiler_errors.borrow();
        if let Some(file_errors) = errors.get(file_path) {
            for spanned_error in file_errors {
                let diagnostic = Diagnostic {
                    range: spanned_error.range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: None,
                    code_description: None,
                    source: Some("datex".to_string()),
                    message: format!("{}", spanned_error.error),
                    related_information: None,
                    tags: None,
                    data: None,
                };
                diagnostics.push(diagnostic);
            }
        }
        diagnostics
    }
}
