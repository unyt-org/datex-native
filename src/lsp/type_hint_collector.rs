use std::ops::Range;
use datex_core::ast::binding::VariableId;
use datex_core::ast::data::expression::VariableDeclaration;
use datex_core::ast::data::visitor::{Visit, Visitable};

#[derive(Default)]
pub struct TypeHintCollector {
    pub type_hints: Vec<(usize, VariableId)>
}

impl Visit for TypeHintCollector {
    fn visit_variable_declaration(&mut self, var_decl: &VariableDeclaration, span: &Range<usize>) {
        if var_decl.type_annotation.is_none() {
            let expr_start = var_decl.init_expression.span.start;
            self.type_hints.push((expr_start - 3, var_decl.id.unwrap()));
        }
        var_decl.visit_children_with(self);
    }
}