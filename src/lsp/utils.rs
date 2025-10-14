use std::path::PathBuf;
use datex_core::ast::tree::{DatexExpression, DatexExpressionData, SimpleSpan, Statements, VariableAccess, VariableAssignment, VariableDeclaration, Visit, Visitable};
use datex_core::compiler::error::CompilerError;
use datex_core::compiler::precompiler::VariableMetadata;
use datex_core::values::core_values::decimal::Decimal;
use datex_core::values::core_values::decimal::typed_decimal::TypedDecimal;
use datex_core::values::core_values::endpoint::Endpoint;
use datex_core::values::core_values::integer::Integer;
use datex_core::values::core_values::integer::typed_integer::TypedInteger;
use realhydroper_lsp::lsp_types::{MessageType, Position, Range, TextDocumentPositionParams};
use crate::lsp::{LanguageServerBackend, SpannedCompilerError};

impl LanguageServerBackend {

    pub async fn update_file_contents(
        &self,
        path: PathBuf,
        content: String,
    ) {
        let mut compiler_workspace = self.compiler_workspace.borrow_mut();
        let file = compiler_workspace.load_file(path.clone(), content.clone());
        // Clear previous errors for this file
        self.clear_compiler_errors(&path);
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
            // Clear previous errors for this file
        } else {
            let error = file.err().unwrap();
            self.client
                .log_message(
                    MessageType::ERROR,
                    format!(
                        "Failed to compile file {}: {}",
                        path.to_str().unwrap(),
                        error,
                    ),
                )
                .await;
            // Collect new errors
            self.collect_compiler_errors(error, &path, &content)
        }
    }
    
    /// Clears all compiler errors associated with the given file path.
    fn clear_compiler_errors(&self, path: &PathBuf) {
        let mut spanned_compiler_errors = self.spanned_compiler_errors.borrow_mut();
        spanned_compiler_errors.remove(path);
    }

    /// Recursively collects spanned compiler errors into the spanned_compiler_errors field.
    fn collect_compiler_errors(&self, compiler_error: CompilerError, path: &PathBuf, file_content: &String) {
        match compiler_error {
            CompilerError::Multiple(errors) => {
                for err in errors {
                    self.collect_compiler_errors(err, path, file_content);
                }
            }
            CompilerError::Spanned(err, span) => {
                let mut spanned_compiler_errors = self.spanned_compiler_errors.borrow_mut();
                let file_errors = spanned_compiler_errors.entry(path.clone()).or_insert_with(Vec::new);
                file_errors.push(SpannedCompilerError {
                    range: self.convert_byte_range_to_document_range(span, file_content),
                    error: *err
                });
            }
            // workaround for now: if not spanned compiler error, just span the whole file
            _ => {
                let mut spanned_compiler_errors = self.spanned_compiler_errors.borrow_mut();
                let file_errors = spanned_compiler_errors.entry(path.clone()).or_insert_with(Vec::new);
                file_errors.push(SpannedCompilerError {
                    range: self.convert_byte_range_to_document_range(0..file_content.len(), file_content),
                    error: compiler_error
                });
            }
        }
    }

    /// Finds all variables in the workspace whose names start with the given prefix.
    pub fn find_variable_starting_with(&self, prefix: &str) -> Vec<VariableMetadata> {
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
    pub fn get_variable_by_id(&self, id: usize) -> Option<VariableMetadata> {
        let compiler_workspace = self.compiler_workspace.borrow();
        for file in compiler_workspace.files().values() {
            let metadata = file.ast_with_metadata.metadata.borrow();
            if let Some(v) = metadata.variables.get(id).cloned() {
                return Some(v);
            }
        }
        None
    }

    /// Converts an LSP position (line and character) to a byte offset in the file content.
    fn position_to_byte_offset(&self, position: &TextDocumentPositionParams) -> usize {
        let workspace = self.compiler_workspace.borrow();
        // first get file contents at position.text_document.uri
        // then calculate byte offset from position.position.line and position.position.character
        let file_path = position.text_document.uri.to_file_path().unwrap();
        let file_content = &workspace.get_file(&file_path).unwrap().content;

        Self::line_char_to_byte_index(file_content, position.position.line as usize, position.position.character as usize).unwrap_or(0)
    }

    /// Converts a byte range (start, end) to a document Range (start Position, end Position) in the file content.
    fn convert_byte_range_to_document_range(&self, span: std::ops::Range<usize>, file_content: &String) -> Range {
        let start = self.byte_offset_to_position(span.start, file_content).unwrap_or(Position { line: 0, character: 0 });
        let end = self.byte_offset_to_position(span.end, file_content).unwrap_or(Position { line: 0, character: 0 });
        Range { start, end }
    }

    /// Converts a byte offset to an LSP position (line and character) in the file content.
    /// TODO: check if this is correct, generated with copilot
    fn byte_offset_to_position(&self, byte_offset: usize, file_content: &String) -> Option<Position> {
        let mut current_offset = 0;
        for (line_idx, line) in file_content.lines().enumerate() {
            let line_length = line.len() + 1; // +1 for the newline character
            if current_offset + line_length > byte_offset {
                // The byte offset is within this line
                let char_offset = line.char_indices()
                    .find(|(i, _)| current_offset + i >= byte_offset)
                    .map(|(i, _)| i)
                    .unwrap_or(line.len());
                return Some(Position {
                    line: line_idx as u32,
                    character: char_offset as u32,
                });
            }
            current_offset += line_length;
        }
        None
    }

    /// Retrieves the text immediately preceding the given position in the document.
    /// This is used for autocompletion suggestions.
    pub fn get_previous_text_at_position(&self, position: &TextDocumentPositionParams) -> String {
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
    pub fn get_expression_at_position(&self, position: &TextDocumentPositionParams) -> DatexExpression {
        let byte_offset = self.position_to_byte_offset(position);
        let workspace = self.compiler_workspace.borrow();
        let file_path = position.text_document.uri.to_file_path().unwrap();
        let ast = &workspace.get_file(&file_path).unwrap().ast_with_metadata.ast;

        let mut finder = ExpressionFinder::new(byte_offset);
        finder.visit_expression(ast.as_ref().unwrap());
        finder.found_expr.unwrap()
    }


    /// Converts a (line, character) pair to a byte index in the given text.
    /// Lines and characters are zero-indexed.
    /// Returns None if the line or character is out of bounds.
    pub fn line_char_to_byte_index(text: &str, line: usize, character: usize) -> Option<usize> {
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


/// Visitor that finds the most specific DatexExpression containing a given byte position.
/// If multiple expressions contain the position, the one with the smallest span is chosen.
struct ExpressionFinder {
    pub search_pos: usize,
    pub found_expr: Option<DatexExpression>,
}

impl ExpressionFinder {
    pub fn new(search_pos: usize) -> Self {
        Self {
            search_pos,
            found_expr: None,
        }
    }


    /// Checks if the given span includes the search position.
    /// If it does, updates found_expr if this expression is more specific (smaller span).
    /// Returns true if the span includes the search position, false otherwise.
    fn match_span(&mut self, span: SimpleSpan, expr: DatexExpression) -> bool {
        if span.start <= self.search_pos && self.search_pos <= span.end {
            // If we already found an expression, only replace it if this one is smaller (more specific)
            if let Some(existing_expr) = &self.found_expr {
                if (span.end - span.start) < (existing_expr.span.end - existing_expr.span.start) {
                    self.found_expr = Some(expr);
                }
            } else {
                self.found_expr = Some(expr);
            }
            true
        }
        else {
            false
        }
    }
}

impl Visit for ExpressionFinder {
    fn visit_statements(&mut self, stmts: &Statements, span: SimpleSpan) {
        if self.match_span(span, DatexExpression {
            data: DatexExpressionData::Statements(stmts.clone()),
            span,
        }) {
            // Only visit children if the span matched, to find more specific expressions within
            stmts.visit_children_with(self);
        }
    }

    fn visit_variable_declaration(&mut self, var_decl: &VariableDeclaration, span: SimpleSpan) {
        if self.match_span(span, DatexExpression {
            data: DatexExpressionData::VariableDeclaration(var_decl.clone()),
            span,
        }) {
            // Also visit the init expression to find more specific expressions within it
            self.visit_expression(&var_decl.init_expression);
        }
    }

    fn visit_variable_assignment(&mut self, var_assign: &VariableAssignment, span: SimpleSpan) {
        if self.match_span(span, DatexExpression {
            data: DatexExpressionData::VariableAssignment(var_assign.clone()),
            span,
        }) {
            // Also visit the assigned expression to find more specific expressions within it
            self.visit_expression(&var_assign.expression);
        }
    }

    fn visit_variable_access(&mut self, var_access: &VariableAccess, span: SimpleSpan) {
        self.match_span(span, DatexExpression {
            data: DatexExpressionData::VariableAccess(var_access.clone()),
            span,
        });
    }

    fn visit_integer(&mut self, value: &Integer, span: SimpleSpan) {
        self.match_span(span, DatexExpression {
            data: DatexExpressionData::Integer(value.clone()),
            span,
        });
    }

    fn visit_typed_integer(&mut self, value: &TypedInteger, span: SimpleSpan) {
        self.match_span(span, DatexExpression {
            data: DatexExpressionData::TypedInteger(value.clone()),
            span,
        });
    }

    fn visit_decimal(&mut self, value: &Decimal, span: SimpleSpan) {
        self.match_span(span, DatexExpression {
            data: DatexExpressionData::Decimal(value.clone()),
            span,
        });
    }

    fn visit_typed_decimal(&mut self, value: &TypedDecimal, span: SimpleSpan) {
        self.match_span(span, DatexExpression {
            data: DatexExpressionData::TypedDecimal(value.clone()),
            span,
        });
    }

    fn visit_text(&mut self, value: &String, span: SimpleSpan) {
        self.match_span(span, DatexExpression {
            data: DatexExpressionData::Text(value.clone()),
            span,
        });
    }

    fn visit_boolean(&mut self, value: bool, span: SimpleSpan) {
        self.match_span(span, DatexExpression {
            data: DatexExpressionData::Boolean(value),
            span,
        });
    }

    fn visit_endpoint(&mut self, value: &Endpoint, span: SimpleSpan) {
        self.match_span(span, DatexExpression {
            data: DatexExpressionData::Endpoint(value.clone()),
            span,
        });
    }

    fn visit_null(&mut self, span: SimpleSpan) {
        self.match_span(span, DatexExpression {
            data: DatexExpressionData::Null,
            span,
        });
    }
}