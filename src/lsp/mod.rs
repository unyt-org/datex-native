use std::cell::RefCell;
use std::sync::Mutex;
use datex_core::compiler::workspace::CompilerWorkspace;
use realhydroper_lsp::jsonrpc::Result;
use realhydroper_lsp::lsp_types::*;
use realhydroper_lsp::{Client, LanguageServer, LspService, Server};

pub struct LanguageServerBackend {
    pub client: Client,
    pub compiler_workspace: RefCell<CompilerWorkspace>,
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
        let mut compiler_workspace = self.compiler_workspace.borrow_mut();
        let file = compiler_workspace.load_file(
            params.text_document.uri.to_file_path().unwrap(),
            params.text_document.text
        ).unwrap();
        self.client
            .log_message(
                MessageType::INFO,
                format!("File compiled, DXB size: {}", file.compiled_dxb.as_ref().map_or(0, |dxb| dxb.len())),
            )
            .await;
        self.client
            .log_message(
                MessageType::INFO,
                format!("AST: {:#?}", file.ast_with_metadata.ast),
            )
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        self.client
            .log_message(MessageType::INFO, "hover!")
            .await;

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: "# Example\n123".to_string(),
            }),
            range: None,
        }))
    }
}