use super::artifacts::legacy_program;
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::Visit;

struct RustCode<'a> {
    bindings: Vec<BTreeMap<String, &'a syn::Expr>>,
    aliases: Vec<BTreeMap<String, String>>,
    violations: BTreeSet<&'static str>,
}
impl<'a> RustCode<'a> {
    fn push_scope(&mut self, items: impl IntoIterator<Item = &'a syn::Item>) {
        self.bindings.push(BTreeMap::new());
        self.aliases.push(BTreeMap::new());
        for item in items {
            match item {
                syn::Item::Const(value) => {
                    self.bindings
                        .last_mut()
                        .unwrap()
                        .insert(value.ident.to_string(), &value.expr);
                }
                syn::Item::Static(value)
                    if matches!(value.mutability, syn::StaticMutability::None) =>
                {
                    self.bindings
                        .last_mut()
                        .unwrap()
                        .insert(value.ident.to_string(), &value.expr);
                }
                syn::Item::Type(alias) => {
                    if let syn::Type::Path(path) = &*alias.ty {
                        self.aliases.last_mut().unwrap().insert(
                            alias.ident.to_string(),
                            path.path
                                .segments
                                .iter()
                                .map(|s| s.ident.to_string())
                                .collect::<Vec<_>>()
                                .join("::"),
                        );
                    }
                }
                syn::Item::Use(import) => self.import(&import.tree, String::new()),
                _ => {}
            }
        }
    }
    fn pop_scope(&mut self) {
        self.bindings.pop();
        self.aliases.pop();
    }
    fn command_type(&self, name: &str, depth: usize) -> bool {
        if depth > 12 {
            return false;
        }
        if name == "Command" || name.ends_with("process::Command") {
            return true;
        }
        self.aliases
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .is_some_and(|alias| self.command_type(alias, depth + 1))
    }
    fn values(&self, expr: &'a syn::Expr, depth: usize) -> Vec<String> {
        if depth > 12 {
            return Vec::new();
        }
        match expr {
            syn::Expr::Lit(e) => match &e.lit {
                syn::Lit::Str(s) => vec![s.value()],
                _ => Vec::new(),
            },
            syn::Expr::Reference(e) => self.values(&e.expr, depth + 1),
            syn::Expr::Paren(e) => self.values(&e.expr, depth + 1),
            syn::Expr::Group(e) => self.values(&e.expr, depth + 1),
            syn::Expr::Path(e) => {
                if let Some(name) = e.path.get_ident() {
                    if let Some(value) = self
                        .bindings
                        .iter()
                        .rev()
                        .find_map(|scope| scope.get(&name.to_string()))
                    {
                        return self.values(value, depth + 1);
                    }
                    return vec![name.to_string()];
                }
                Vec::new()
            }
            syn::Expr::Field(e) => match &e.member {
                syn::Member::Named(n) => vec![n.to_string()],
                _ => Vec::new(),
            },
            syn::Expr::Call(e) => e
                .args
                .iter()
                .flat_map(|e| self.values(e, depth + 1))
                .collect(),
            syn::Expr::MethodCall(e) => self
                .values(&e.receiver, depth + 1)
                .into_iter()
                .chain(e.args.iter().flat_map(|e| self.values(e, depth + 1)))
                .collect(),
            syn::Expr::Index(e) => self.first(&e.expr, depth + 1),
            _ => Vec::new(),
        }
    }
    fn first(&self, expr: &'a syn::Expr, depth: usize) -> Vec<String> {
        if depth > 12 {
            return Vec::new();
        }
        match expr {
            syn::Expr::Reference(e) => self.first(&e.expr, depth + 1),
            syn::Expr::Array(e) => e
                .elems
                .first()
                .map(|e| self.values(e, depth + 1))
                .unwrap_or_default(),
            syn::Expr::Path(e) => e
                .path
                .get_ident()
                .and_then(|n| {
                    self.bindings
                        .iter()
                        .rev()
                        .find_map(|s| s.get(&n.to_string()))
                })
                .map(|e| self.first(e, depth + 1))
                .unwrap_or_default(),
            syn::Expr::Macro(e) if e.mac.path.is_ident("vec") => {
                let parse = e.mac.parse_body_with(
                    syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated,
                );
                // Macro-owned nodes cannot borrow the original tree; only static strings are needed.
                parse
                    .ok()
                    .and_then(|values| {
                        values.first().and_then(|e| {
                            if let syn::Expr::Lit(e) = e
                                && let syn::Lit::Str(s) = &e.lit
                            {
                                return Some(vec![s.value()]);
                            }
                            None
                        })
                    })
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }
    fn check_program(&mut self, values: Vec<String>) {
        if values.iter().any(|v| {
            legacy_program(v)
                || matches!(
                    v.as_str(),
                    "PYO3_PYTHON" | "PYTHON_EXECUTABLE" | "POWERSHELL_EXE"
                )
        }) {
            self.violations
                .insert("actual Rust command hands execution to a retired language interpreter");
        }
    }
    fn import(&mut self, tree: &syn::UseTree, prefix: String) {
        let first = match tree {
            syn::UseTree::Path(p) => Some(&p.ident),
            syn::UseTree::Name(p) => Some(&p.ident),
            syn::UseTree::Rename(p) => Some(&p.ident),
            _ => None,
        };
        if prefix.is_empty()
            && first.is_some_and(|n| {
                matches!(
                    n.to_string().as_str(),
                    "pyo3" | "cpython" | "rustpython" | "rustpython_vm"
                )
            })
        {
            self.violations
                .insert("first-party Rust retains a Python interpreter binding");
        }
        match tree {
            syn::UseTree::Path(p) => self.import(&p.tree, format!("{prefix}{}::", p.ident)),
            syn::UseTree::Name(p) => {
                self.aliases
                    .last_mut()
                    .unwrap()
                    .insert(p.ident.to_string(), format!("{prefix}{}", p.ident));
            }
            syn::UseTree::Rename(p) => {
                self.aliases
                    .last_mut()
                    .unwrap()
                    .insert(p.rename.to_string(), format!("{prefix}{}", p.ident));
            }
            syn::UseTree::Group(p) => {
                for item in &p.items {
                    self.import(item, prefix.clone());
                }
            }
            syn::UseTree::Glob(_) => {}
        }
    }
}
impl<'a> Visit<'a> for RustCode<'a> {
    fn visit_file(&mut self, file: &'a syn::File) {
        self.push_scope(&file.items);
        syn::visit::visit_file(self, file);
        self.pop_scope();
    }
    fn visit_item_mod(&mut self, module: &'a syn::ItemMod) {
        if let Some((_, items)) = &module.content {
            self.push_scope(items);
            syn::visit::visit_item_mod(self, module);
            self.pop_scope();
        } else {
            syn::visit::visit_item_mod(self, module);
        }
    }
    fn visit_block(&mut self, block: &'a syn::Block) {
        self.push_scope(block.stmts.iter().filter_map(|stmt| {
            if let syn::Stmt::Item(item) = stmt {
                Some(item)
            } else {
                None
            }
        }));
        syn::visit::visit_block(self, block);
        self.pop_scope();
    }
    fn visit_local(&mut self, node: &'a syn::Local) {
        if let syn::Pat::Ident(name) = &node.pat
            && name.mutability.is_none()
            && let Some(initial) = &node.init
            && let Some(scope) = self.bindings.last_mut()
        {
            scope.insert(name.ident.to_string(), &initial.expr);
        }
        syn::visit::visit_local(self, node);
    }
    fn visit_item_use(&mut self, node: &'a syn::ItemUse) {
        self.import(&node.tree, String::new());
        syn::visit::visit_item_use(self, node);
    }
    fn visit_path(&mut self, path: &'a syn::Path) {
        if path.segments.first().is_some_and(|s| {
            matches!(
                s.ident.to_string().as_str(),
                "pyo3" | "cpython" | "rustpython" | "rustpython_vm"
            )
        }) {
            self.violations
                .insert("first-party Rust retains a Python interpreter binding");
        }
        syn::visit::visit_path(self, path);
    }
    fn visit_expr_call(&mut self, node: &'a syn::ExprCall) {
        if let syn::Expr::Path(path) = &*node.func {
            let names = path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>();
            let joined = names.join("::");
            let constructor = names.last().is_some_and(|s| s == "new")
                && names.len() > 1
                && self.command_type(&names[..names.len() - 1].join("::"), 0);
            if constructor && let Some(program) = node.args.first() {
                self.check_program(self.values(program, 0));
            }
            if joined.ends_with("tooling_process::capture")
                || joined.ends_with("tooling_process::capture_with_input")
            {
                if let Some(argv) = node.args.iter().nth(1) {
                    self.check_program(self.first(argv, 0));
                }
            } else if (joined.ends_with("codec_process::capture")
                || joined.ends_with("codec_process::capture_with_budget"))
                && let Some(program) = node.args.iter().nth(1)
            {
                self.check_program(self.values(program, 0));
            }
        }
        syn::visit::visit_expr_call(self, node);
    }
}
pub(super) fn rust_violations(text: &str) -> Result<BTreeSet<&'static str>, String> {
    let file =
        syn::parse_file(text).map_err(|e| format!("cannot parse first-party Rust source: {e}"))?;
    let mut visitor = RustCode {
        bindings: Vec::new(),
        aliases: Vec::new(),
        violations: BTreeSet::new(),
    };
    visitor.visit_file(&file);
    Ok(visitor.violations)
}
