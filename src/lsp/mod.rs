use std::cell::RefCell;
use std::path::PathBuf;
use datex_core::ast::tree::{DatexExpression, DatexExpressionData};
use datex_core::compiler::precompiler::VariableMetadata;
use datex_core::compiler::workspace::CompilerWorkspace;
use realhydroper_lsp::jsonrpc::Result;
use realhydroper_lsp::lsp_types::*;
use realhydroper_lsp::{Client, LanguageServer, LspService, Server};

pub struct LanguageServerBackend {
    pub client: Client,
    pub compiler_workspace: RefCell<CompilerWorkspace>,
}

impl LanguageServerBackend {

    async fn update_file_contents(
        &self,
        path: PathBuf,
        content: String,
    ) {
        let mut compiler_workspace = self.compiler_workspace.borrow_mut();
        let file = compiler_workspace.load_file(path.clone(), content);
        if let Ok(file) = file {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("AST: {:#?}", file.ast_with_metadata.ast),
                )
                .await;
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("AST metadata: {:#?}", *file.ast_with_metadata.metadata.borrow())
                )
                .await;
        } else {
            self.client
                .log_message(
                    MessageType::ERROR,
                    format!(
                        "Failed to compile file {}: {}",
                        path.to_str().unwrap(),
                        file.err().unwrap()
                    ),
                )
                .await;
        }
    }

    /// Finds all variables in the workspace whose names start with the given prefix.
    fn find_variable_starting_with(&self, prefix: &str) -> Vec<VariableMetadata> {
        let compiler_workspace = self.compiler_workspace.borrow();
        let mut results = Vec::new();
        for file in compiler_workspace.files().values() {
            let metadata = file.ast_with_metadata.metadata.borrow();
            for var in metadata.variables.iter() {
                if var.name.starts_with(prefix) {
                    results.push(var.clone());
                }
            }
        }
        results
    }

    /// Retrieves variable metadata by its unique ID.
    fn get_variable_by_id(&self, id: usize) -> Option<VariableMetadata> {
        let compiler_workspace = self.compiler_workspace.borrow();
        for file in compiler_workspace.files().values() {
            let metadata = file.ast_with_metadata.metadata.borrow();
            if let Some(v) = metadata.variables.get(id).cloned() {
                return Some(v);
            }
        }
        None
    }

    /// Converts a LSP position (line and character) to a byte offset in the file content.
    fn position_to_byte_offset(&self, position: &TextDocumentPositionParams) -> usize {
        let workspace = self.compiler_workspace.borrow();
        // first get file contents at position.text_document.uri
        // then calculate byte offset from position.position.line and position.position.character
        let file_path = position.text_document.uri.to_file_path().unwrap();
        let file_content = &workspace.get_file(&file_path).unwrap().content;

        Self::line_char_to_byte_index(file_content, position.position.line as usize, position.position.character as usize).unwrap_or(0)
    }

    fn get_previous_text_at_position(&self, position: &TextDocumentPositionParams) -> String {
        let byte_offset = self.position_to_byte_offset(position);
        let workspace = self.compiler_workspace.borrow();
        let file_path = position.text_document.uri.to_file_path().unwrap();
        let file_content = &workspace.get_file(&file_path).unwrap().content;
        // Get the text before the byte offset, only matching word characters
        let previous_text = &file_content[..byte_offset];
        let last_word = previous_text
            .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap_or("");
        last_word.to_string()
    }

    /// Retrieves the DatexExpression AST node at the given byte offset.
    fn get_expression_at_position(&self, position: &TextDocumentPositionParams) -> DatexExpression {
        let byte_offset = self.position_to_byte_offset(position);
        let workspace = self.compiler_workspace.borrow();
        let file_path = position.text_document.uri.to_file_path().unwrap();
        let ast = &workspace.get_file(&file_path).unwrap().ast_with_metadata.ast;
        ast.as_ref().cloned().unwrap()
    }


    /// Converts a (line, character) pair to a byte index in the given text.
    /// Lines and characters are zero-indexed.
    /// Returns None if the line or character is out of bounds.
    fn line_char_to_byte_index(text: &str, line: usize, character: usize) -> Option<usize> {
        let mut lines = text.split('\n');

        // Get the line
        let line_text = lines.nth(line)?;

        // Compute byte index of the start of that line
        let byte_offset_to_line_start = text
            .lines()
            .take(line)
            .map(|l| l.len() + 1) // +1 for '\n'
            .sum::<usize>();

        // Now find the byte index within that line for the given character offset
        let byte_offset_within_line = line_text
            .char_indices()
            .nth(character)
            .map(|(i, _)| i)
            .unwrap_or_else(|| line_text.len());

        Some(byte_offset_to_line_start + byte_offset_within_line)
    }
}

#[realhydroper_lsp::async_trait(?Send)]
impl LanguageServer for LanguageServerBackend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions::default()),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
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

    async fn shutdown(&self) -> Result<()> {
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
        ).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("File changed: {}", params.text_document.uri),
            )
            .await;
        let new_content = params.content_changes.into_iter().next().map(|change| change.text).unwrap_or_default();
        self.update_file_contents(
            params.text_document.uri.to_file_path().unwrap(),
            new_content,
        ).await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
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

        let items: Vec<CompletionItem> = variables.iter().map(|var| {
            CompletionItem {
                label: var.name.clone(),
                kind: Some(CompletionItemKind::VARIABLE),
                detail: Some(format!(
                    "{} {}: {}",
                    var.shape,
                    var.name,
                    var.var_type.as_ref().unwrap())
                ),
                documentation: None,
                ..Default::default()
            }
        }).collect();

        Ok(Some(CompletionResponse::Array(items)))
    }


    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let expression = self.get_expression_at_position(&params.text_document_position_params);

        match expression {
            // show variable type info on hover
            DatexExpression { data: DatexExpressionData::VariableDeclaration { name, id: Some(id), .. }, .. } |
            DatexExpression { data: DatexExpressionData::VariableAssignment (_, Some(id), name, _), .. } |
            DatexExpression { data: DatexExpressionData::Variable (id, name), .. } => {
                let variable_metadata = self.get_variable_by_id(id).unwrap();
                let contents = HoverContents::Scalar(MarkedString::LanguageString(LanguageString {
                    language: "datex".to_string(),
                    value: format!(
                        "{} {}: {}",
                        variable_metadata.shape,
                        name,
                        variable_metadata.var_type.as_ref().unwrap()
                    )
                }));
                Ok(Some(Hover { contents, range: None }))
            }
            _ => Ok(None)
        }
    }
}