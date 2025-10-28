use std::ops::Range;
use datex_core::ast::data::expression::VariableDeclaration;
use datex_core::ast::data::visitor::{Visit, Visitable};

#[derive(Default)]
pub struct VariableDeclarationFinder {
    pub var_id: usize,
    pub variable_declaration_position: Option<Range<usize>>
}

impl VariableDeclarationFinder {
    pub fn new(var_id: usize) -> Self {
        VariableDeclarationFinder { var_id, variable_declaration_position: None }
    }
}

impl Visit for VariableDeclarationFinder {
    fn visit_variable_declaration(&mut self, var_decl: &VariableDeclaration, span: &Range<usize>) {
        if var_decl.id == Some(self.var_id) {
            self.variable_declaration_position = Some(span.clone());
        }
        else {
            var_decl.visit_children_with(self);
        }
    }
}